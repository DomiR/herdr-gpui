import CryptoKit
import Foundation

extension NotificationSeal {
    /// Seals `plaintext` to `recipient` under `info`, as
    /// `crates/sheprd-apns/src/seal.rs` does: the layout `open` reads, with a
    /// one-time key and nonce. The phone seals its push relay handles this
    /// way, to the push relay's key. Tests pass fixed one-time values;
    /// everything else takes fresh random ones.
    public static func seal(
        _ plaintext: Data,
        to recipient: P256.KeyAgreement.PublicKey,
        info: Data,
        ephemeral: P256.KeyAgreement.PrivateKey = P256.KeyAgreement.PrivateKey(),
        nonce: AES.GCM.Nonce = AES.GCM.Nonce()
    ) throws -> Data {
        let point = ephemeral.publicKey.x963Representation
        let key = try ephemeral.sharedSecretFromKeyAgreement(with: recipient).hkdfDerivedSymmetricKey(
            using: SHA256.self,
            salt: point + recipient.x963Representation,
            sharedInfo: info,
            outputByteCount: 32
        )
        let box = try AES.GCM.seal(plaintext, using: key, nonce: nonce, authenticating: info)
        return point + Data(nonce) + box.ciphertext + box.tag
    }
}
