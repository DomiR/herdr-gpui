import HerdrRemoteCore
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The chat's composer: text with `/` completion, pictures from the
/// library, the camera, or the clipboard, dictation on the phone, and
/// send or stop.
struct Composer: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let transcript: Transcript
    @Binding var problem: String?
    @State private var draft = ""
    @State private var pictures: [Picture] = []
    @State private var picks: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var takingPhoto = false
    @State private var clipboardHasImage = UIPasteboard.general.hasImages
    @State private var dictation = Dictation()
    /// The draft before dictation began, which dictated words follow.
    @State private var beforeDictation = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 8) {
            if !completions.isEmpty { completionList }
            if !pictures.isEmpty || (clipboardHasImage && focused) { pictureStrip }
            if case let .failed(reason) = dictation.phase {
                Text(reason).font(.footnote).foregroundStyle(.secondary)
            }
            HStack(alignment: .bottom, spacing: 10) {
                attachMenu
                TextField("Message \(transcript.agent)", text: $draft, axis: .vertical)
                    .lineLimit(1...6)
                    .focused($focused)
                    .padding(.horizontal, 16).padding(.vertical, 11)
                    .glassEffect(.regular, in: .rect(cornerRadius: 22))
                trailingButton
            }
        }
        .padding(.horizontal, 16).padding(.bottom, 6)
        .photosPicker(isPresented: $choosingPhotos, selection: $picks, maxSelectionCount: 4, matching: .images)
        .fullScreenCover(isPresented: $takingPhoto) {
            CameraPicker { image in if let image { add(image) } }.ignoresSafeArea()
        }
        .onChange(of: picks) { _, chosen in
            picks = []
            for item in chosen {
                Task {
                    if let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data) {
                        add(image)
                    }
                }
            }
        }
        .onChange(of: dictation.text) { _, words in
            guard dictation.active || !words.isEmpty else { return }
            draft = beforeDictation.isEmpty ? words : beforeDictation + " " + words
        }
        .onReceive(NotificationCenter.default.publisher(for: UIPasteboard.changedNotification)) { _ in
            clipboardHasImage = UIPasteboard.general.hasImages
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            clipboardHasImage = UIPasteboard.general.hasImages
        }
        .sensoryFeedback(.start, trigger: dictation.phase == .listening) { _, now in now }
        #if DEBUG
        // The simulator loop's `-draft TEXT` types into the composer.
        .onAppear { if let text = UserDefaults.standard.string(forKey: "draft") { draft = text } }
        #endif
    }

    // MARK: Parts

    private var attachMenu: some View {
        Menu {
            Button("Photo Library", systemImage: "photo.on.rectangle") { choosingPhotos = true }
            if UIImagePickerController.isSourceTypeAvailable(.camera) {
                Button("Take Photo", systemImage: "camera") { takingPhoto = true }
            }
        } label: {
            Image(systemName: "plus").font(.body.bold()).frame(width: 36, height: 36)
        }
        .buttonStyle(.glass)
        .buttonBorderShape(.circle)
        .disabled(transcript.readOnly || pictures.count >= 4)
        .accessibilityLabel("Add a picture")
    }

    @ViewBuilder
    private var trailingButton: some View {
        let stopping = draft.isEmpty && pictures.isEmpty && transcript.status == .working
        if dictation.active {
            Button {
                Task { await dictation.stop() }
            } label: {
                Image(systemName: dictation.phase == .preparing ? "ellipsis" : "stop.circle.fill")
                    .font(.body.bold()).frame(width: 36, height: 36)
                    .symbolEffect(.pulse, isActive: dictation.phase == .listening)
            }
            .buttonStyle(.glassProminent).buttonBorderShape(.circle).tint(.red)
            .accessibilityLabel("Stop dictating")
        } else if draft.isEmpty && pictures.isEmpty && !stopping {
            Button {
                beforeDictation = draft
                Task { await dictation.start() }
            } label: {
                Image(systemName: "mic.fill").font(.body.bold()).frame(width: 36, height: 36)
            }
            .buttonStyle(.glass).buttonBorderShape(.circle)
            .disabled(transcript.readOnly)
            .accessibilityLabel("Dictate")
        } else {
            Button {
                if stopping { send(.cancel) } else { sendPrompt() }
            } label: {
                Image(systemName: stopping ? "stop.fill" : "arrow.up").font(.body.bold()).frame(width: 36, height: 36)
            }
            .buttonStyle(.glassProminent).buttonBorderShape(.circle)
            .disabled(transcript.readOnly || pictures.contains { $0.upload == .sending })
            .accessibilityLabel(stopping ? "Stop" : "Send")
        }
    }

    private var pictureStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(pictures) { picture in
                    Image(uiImage: picture.thumbnail).resizable().scaledToFill()
                        .frame(width: 56, height: 56).clipShape(.rect(cornerRadius: 10))
                        .overlay {
                            switch picture.upload {
                            case .sending: ProgressView().tint(.white)
                            case .failed: Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
                            case .sent: EmptyView()
                            }
                        }
                        .overlay(alignment: .topTrailing) {
                            Button {
                                pictures.removeAll { $0.id == picture.id }
                            } label: {
                                Image(systemName: "xmark.circle.fill").symbolRenderingMode(.palette)
                                    .foregroundStyle(.white, .black.opacity(0.6))
                            }
                            .offset(x: 6, y: -6)
                            .accessibilityLabel("Remove picture")
                        }
                }
                if clipboardHasImage, pictures.count < 4 {
                    PasteButton(payloadType: PastedImage.self) { pasted in
                        for image in pasted.prefix(4 - pictures.count) { add(image.image) }
                    }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.roundedRectangle(radius: 10))
                }
            }
            .padding(.top, 6).padding(.horizontal, 2)
        }
    }

    private var completions: [SlashCommand] {
        guard draft.hasPrefix("/"), !draft.contains(where: \.isWhitespace) else { return [] }
        let typed = draft.dropFirst().lowercased()
        return Array(transcript.commands.filter { typed.isEmpty || $0.name.lowercased().hasPrefix(typed) }.prefix(6))
    }

    private var completionList: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(completions, id: \.name) { command in
                Button {
                    draft = "/\(command.name) "
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text("/\(command.name)").font(.subheadline.monospaced().weight(.semibold))
                            if let hint = command.hint { Text(hint).font(.caption).foregroundStyle(.tertiary) }
                        }
                        if !command.description.isEmpty {
                            Text(command.description).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14).padding(.vertical, 8)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, 4)
        .glassEffect(.regular, in: .rect(cornerRadius: 18))
    }

    // MARK: Actions

    /// Shrinks a picture to what the daemon takes and uploads it at once,
    /// so sending does not wait on it.
    private func add(_ image: UIImage) {
        guard pictures.count < 4, let prepared = PreparedImage(image) else { return }
        let picture = Picture(thumbnail: prepared.thumbnail, jpeg: prepared.jpeg)
        pictures.append(picture)
        upload(picture.id)
    }

    private func upload(_ id: UUID) {
        guard let jpeg = pictures.first(where: { $0.id == id })?.jpeg else { return }
        let pane = key.pane
        Task {
            let result: Picture.Upload
            do {
                let uploaded = try await model.fleet.call(key.device) { client, token in
                    try await client.upload(jpeg, pane: pane, bearer: token)
                }
                result = .sent(uploaded)
            } catch {
                problem = error.localizedDescription
                result = .failed
            }
            if let index = pictures.firstIndex(where: { $0.id == id }) { pictures[index].upload = result }
        }
    }

    private func sendPrompt() {
        // A failed upload is tried again rather than sent without it.
        for picture in pictures where picture.upload == .failed {
            if let index = pictures.firstIndex(where: { $0.id == picture.id }) { pictures[index].upload = .sending }
            upload(picture.id)
        }
        let ids = pictures.compactMap { if case let .sent(id) = $0.upload { id } else { nil } }
        guard ids.count == pictures.count else { return }
        let text = draft
        let sentPictures = pictures
        draft = ""
        pictures = []
        dictation.reset()
        Task {
            problem = await model.fleet.send(.prompt(text: text, attachments: ids), to: key)
            if problem != nil {
                draft = text
                pictures = sentPictures
            }
        }
    }

    private func send(_ command: ChatCommand) {
        Task { problem = await model.fleet.send(command, to: key) }
    }
}

/// A picture waiting in the composer.
private struct Picture: Identifiable {
    enum Upload: Equatable {
        case sending
        case sent(String)
        case failed
    }

    let id = UUID()
    let thumbnail: UIImage
    let jpeg: Data
    var upload = Upload.sending
}

/// A picture as the daemon takes it: JPEG, at most 2048 points on its
/// long side and under its 3.5 MB limit.
struct PreparedImage {
    static let maxBytes = 3_400_000
    static let maxSide: CGFloat = 2048

    let jpeg: Data
    let thumbnail: UIImage

    init?(_ image: UIImage) {
        let scaled = Self.scaled(image, to: Self.maxSide)
        var quality: CGFloat = 0.82
        var data = scaled.jpegData(compressionQuality: quality)
        while let bytes = data, bytes.count > Self.maxBytes, quality > 0.3 {
            quality -= 0.15
            data = scaled.jpegData(compressionQuality: quality)
        }
        guard let data, data.count <= Self.maxBytes else { return nil }
        jpeg = data
        thumbnail = Self.scaled(image, to: 160)
    }

    static func scaled(_ image: UIImage, to side: CGFloat) -> UIImage {
        let longest = max(image.size.width, image.size.height)
        guard longest > side else { return image }
        let factor = side / longest
        let size = CGSize(width: image.size.width * factor, height: image.size.height * factor)
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }
}

/// An image from the clipboard, through `PasteButton`, which reads it
/// without the paste prompt.
struct PastedImage: Transferable {
    let image: UIImage

    static var transferRepresentation: some TransferRepresentation {
        DataRepresentation(importedContentType: .image) { data in
            guard let image = UIImage(data: data) else { throw CocoaError(.fileReadCorruptFile) }
            return PastedImage(image: image)
        }
    }
}

/// The camera, for a photo to send.
struct CameraPicker: UIViewControllerRepresentable {
    let done: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    @MainActor
    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: (UIImage?) -> Void

        init(done: @escaping (UIImage?) -> Void) { self.done = done }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
            picker.dismiss(animated: true)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            done(nil)
            picker.dismiss(animated: true)
        }
    }
}
