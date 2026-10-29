// herdr-nearby: the Mac's Bluetooth side of nearby pairing
// (docs/ios-app-plan.md). Herdr UI starts it while the pairing dialog is
// open and talks to it in newline-delimited JSON on stdio; it advertises
// the nearby service and carries `pair` frames between phones and Herdr
// UI, which relays them to the daemon. It holds no secrets and decides
// nothing: the code exchange runs in the daemon.
//
// It also registers the `_herdr-remote._tcp` Bonjour service with
// `pairing=1` through the system's DNS-SD (Bonjour.swift).
// `--no-bluetooth` leaves Bluetooth alone: for Macs without it, and for
// tests that must not ask for the permission.
//
// From Herdr UI:  {"type":"advertise","name":"…","fp":"16 hex","port":"7443"}
//                 {"type":"frame","central":"UUID","data":"base64"}
//                 {"type":"stop"}
// To Herdr UI:    {"type":"state","state":"powered_on|powered_off|unauthorized|unsupported|resetting|unknown"}
//                 {"type":"frame","central":"UUID","data":"base64"}
//                 {"type":"error","message":"…"}

import CoreBluetooth
import Foundation

enum IDs {
    static let service = "DD750B9A-9568-4FC3-B357-B7515AE46DED"
    /// Read: `{"v":1,"name":…,"fp":…}`.
    static let info = "820A3154-F1B2-47C2-B8BE-D89E812464A2"
    /// Write and notify: framed JSON messages.
    static let pair = "8D5498FD-CFE2-4D2C-99E9-1FDBC2D83892"
}

func emit(_ message: [String: String]) {
    guard let data = try? JSONSerialization.data(withJSONObject: message) else { return }
    FileHandle.standardOutput.write(data + Data("\n".utf8))
}

/// Peripheral callbacks arrive on the main queue (`queue: nil`); the
/// preconcurrency conformance checks that at run time.
@MainActor
final class Nearby: NSObject, @preconcurrency CBPeripheralManagerDelegate {
    private var manager: CBPeripheralManager!
    private let pair = CBMutableCharacteristic(type: CBUUID(string: IDs.pair), properties: [.write, .notify], value: nil, permissions: [.writeable])
    private let info = CBMutableCharacteristic(type: CBUUID(string: IDs.info), properties: [.read], value: nil, permissions: [.readable])
    private var advertised: (name: String, fingerprint: String)?
    private var serviceAdded = false
    private var incoming: [UUID: Reassembly] = [:]
    private var centrals: [UUID: CBCentral] = [:]
    /// Notifications waiting for the transmit queue to drain.
    private var pending: [(Data, CBCentral)] = []

    func start() {
        manager = CBPeripheralManager(delegate: self, queue: nil)
    }

    func advertise(name: String, fingerprint: String) {
        advertised = (name, fingerprint)
        apply()
    }

    func stop() {
        advertised = nil
        manager.stopAdvertising()
    }

    /// Sends an answer to a phone, chunked to what it can take.
    func send(_ message: Data, to central: UUID) {
        guard let target = centrals[central] else { return }
        for chunk in Framing.chunks(message, size: target.maximumUpdateValueLength) {
            pending.append((chunk, target))
        }
        flush()
    }

    private func flush() {
        while let (chunk, central) = pending.first {
            guard manager.updateValue(chunk, for: pair, onSubscribedCentrals: [central]) else { return }
            pending.removeFirst()
        }
    }

    private func apply() {
        guard manager.state == .poweredOn else { return }
        if !serviceAdded {
            let service = CBMutableService(type: CBUUID(string: IDs.service), primary: true)
            service.characteristics = [info, pair]
            manager.add(service)
            serviceAdded = true
        }
        guard advertised != nil, !manager.isAdvertising else { return }
        // Only the service UUID, which iOS needs to find it in the
        // background; the name stays in `info`.
        manager.startAdvertising([CBAdvertisementDataServiceUUIDsKey: [CBUUID(string: IDs.service)]])
    }

    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        let names: [CBManagerState: String] = [
            .poweredOn: "powered_on", .poweredOff: "powered_off", .unauthorized: "unauthorized",
            .unsupported: "unsupported", .resetting: "resetting",
        ]
        emit(["type": "state", "state": names[peripheral.state] ?? "unknown"])
        apply()
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didAdd service: CBService, error: Error?) {
        if let error { emit(["type": "error", "message": "service: \(error.localizedDescription)"]) }
    }

    func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager, error: Error?) {
        if let error { emit(["type": "error", "message": "advertising: \(error.localizedDescription)"]) }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        guard let advertised else {
            peripheral.respond(to: request, withResult: .readNotPermitted)
            return
        }
        let body = (try? JSONSerialization.data(withJSONObject: ["v": 1, "name": advertised.name, "fp": advertised.fingerprint] as [String: Any])) ?? Data()
        guard request.offset <= body.count else {
            peripheral.respond(to: request, withResult: .invalidOffset)
            return
        }
        request.value = body.subdata(in: request.offset..<body.count)
        peripheral.respond(to: request, withResult: .success)
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        guard advertised != nil else {
            if let first = requests.first { peripheral.respond(to: first, withResult: .writeNotPermitted) }
            return
        }
        for request in requests {
            let central = request.central.identifier
            centrals[central] = request.central
            var assembly = incoming[central] ?? Reassembly()
            switch assembly.add(request.value ?? Data()) {
            case let .complete(message):
                incoming[central] = nil
                emit(["type": "frame", "central": central.uuidString, "data": message.base64EncodedString()])
            case .partial:
                incoming[central] = assembly
            case .invalid:
                incoming[central] = nil
            }
        }
        if let first = requests.first { peripheral.respond(to: first, withResult: .success) }
    }

    func peripheralManagerIsReady(toUpdateSubscribers peripheral: CBPeripheralManager) {
        flush()
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom characteristic: CBCharacteristic) {
        centrals[central.identifier] = nil
        incoming[central.identifier] = nil
    }
}

let bluetooth = !CommandLine.arguments.contains("--no-bluetooth")
let nearby = Nearby()
let bonjour = Bonjour()
if bluetooth { nearby.start() }

// Commands from Herdr UI, one JSON object per line, read off the main
// thread and applied on it. End of input means Herdr UI went away.
Thread.detachNewThread {
    while let line = readLine() {
        guard let object = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: String] else { continue }
        let type = object["type"], name = object["name"], fingerprint = object["fp"]
        let port = object["port"].flatMap { Int($0) } ?? 0
        let central = object["central"].flatMap(UUID.init(uuidString:))
        let data = object["data"].flatMap { Data(base64Encoded: $0) }
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                switch type {
                case "advertise":
                    bonjour.publish(name: name ?? "Mac", fingerprint: fingerprint ?? "", port: port)
                    if bluetooth { nearby.advertise(name: name ?? "Mac", fingerprint: fingerprint ?? "") }
                case "stop":
                    bonjour.stop()
                    if bluetooth { nearby.stop() }
                case "frame": if let central, let data { nearby.send(data, to: central) }
                default: break
                }
            }
        }
    }
    exit(0)
}
RunLoop.main.run()
