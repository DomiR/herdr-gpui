# Checks what the Attest tab saved, the way the push relay will check an
# attestation (docs/apns-relay-plan.md, spike 3). Usage:
#   xcrun devicectl device copy from --device UDID --domain-type appDataContainer \
#     --domain-identifier so.pen.herdr-spikes --source Documents/attest.json --destination attest.json
#   curl -O https://www.apple.com/certificateauthority/Apple_App_Attestation_Root_CA.pem
#   uv run --no-project --with cryptography --with cbor2 python -I ios/Spikes/attest-check.py \
#     attest.json Apple_App_Attestation_Root_CA.pem TEAMID.so.pen.herdr-spikes [UNIX_SECONDS]
# The optional time checks the certificates then rather than now: Apple's
# leaf is good for three days. Exits 1 when any check fails.
import base64, hashlib, json, struct, sys, datetime
import cbor2
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec

rec = json.load(open(sys.argv[1]))
root = x509.load_pem_x509_certificate(open(sys.argv[2], "rb").read())
app_id = sys.argv[3]
b = lambda k: base64.b64decode(rec[k])
key_id, challenge = b("key_id"), b("challenge")
ok = lambda name, cond: print(("PASS " if cond else "FAIL ") + name) or cond
results = []

att = cbor2.loads(b("attestation"))
results.append(ok("format is apple-appattest", att["fmt"] == "apple-appattest"))
cred, inter = [x509.load_der_x509_certificate(c) for c in att["attStmt"]["x5c"]]
now = (
    datetime.datetime.fromtimestamp(int(sys.argv[4]), datetime.timezone.utc)
    if len(sys.argv) > 4
    else datetime.datetime.now(datetime.timezone.utc)
)

def extension(cert, kind):
    try:
        return cert.extensions.get_extension_for_class(kind).value
    except x509.ExtensionNotFound:
        return None

def valid_now(cert):
    return cert.not_valid_before_utc <= now <= cert.not_valid_after_utc

def may_issue(parent, below):
    """Whether `parent` may sign certificates with `below` CAs under it."""
    constraints = extension(parent, x509.BasicConstraints)
    if constraints is None or not constraints.ca:
        return False
    if constraints.path_length is not None and below > constraints.path_length:
        return False
    usage = extension(parent, x509.KeyUsage)
    return usage is None or usage.key_cert_sign

def issued_by(child, parent, below):
    """The whole link: names, the parent's right to sign, the signature, and
    both certificates' dates."""
    if child.issuer != parent.subject or not may_issue(parent, below):
        return False
    try:
        parent.public_key().verify(child.signature, child.tbs_certificate_bytes, ec.ECDSA(child.signature_hash_algorithm))
    except Exception as e:
        print("   ", e)
        return False
    return valid_now(child) and valid_now(parent)

results.append(ok("root is self-signed and valid", issued_by(root, root, 1)))
results.append(ok("intermediate issued by Apple App Attest root", issued_by(inter, root, 0)))
results.append(ok("credential cert issued by intermediate", issued_by(cred, inter, 0)))
cred_constraints = extension(cred, x509.BasicConstraints)
results.append(ok("credential cert is not a CA", cred_constraints is None or not cred_constraints.ca))

auth = att["authData"]
nonce = hashlib.sha256(auth + hashlib.sha256(challenge).digest()).digest()
ext = cred.extensions.get_extension_for_oid(x509.ObjectIdentifier("1.2.840.113635.100.8.2")).value.value
# DER: SEQUENCE { [1] { OCTET STRING (32) nonce } }, exactly.
results.append(ok("nonce in certificate matches the challenge", ext == bytes.fromhex("3024a1220420") + nonce))

pub = cred.public_key()
point = pub.public_bytes(serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)
results.append(ok("key id is SHA-256 of the attested public key", hashlib.sha256(point).digest() == key_id))
results.append(ok(f"rpIdHash is SHA-256 of {app_id}", auth[:32] == hashlib.sha256(app_id.encode()).digest()))
counter = struct.unpack(">I", auth[33:37])[0]
results.append(ok("counter is 0", counter == 0))
aaguid = auth[37:53]
env = {b"appattestdevelop": "development", b"appattest" + b"\0" * 7: "production"}.get(aaguid, f"unknown {aaguid!r}")
print(f"     environment: {env}")
n = struct.unpack(">H", auth[53:55])[0]
results.append(ok("credential id is the key id", auth[55:55 + n] == key_id))
print(f"     receipt: {len(att['attStmt']['receipt'])} bytes; chain: {cred.subject.rfc4514_string()} <- {inter.subject.rfc4514_string()}")

asr = cbor2.loads(b("assertion"))
a_auth = asr["authenticatorData"]
a_nonce = hashlib.sha256(a_auth + hashlib.sha256(b("client_data")).digest()).digest()
try:
    pub.verify(asr["signature"], a_nonce, ec.ECDSA(hashes.SHA256())); sig = True
except Exception:
    sig = False
results.append(ok("assertion signature verifies with the attested key", sig))
results.append(ok("assertion rpIdHash matches", a_auth[:32] == auth[:32]))
a_counter = struct.unpack(">I", a_auth[33:37])[0]
results.append(ok(f"assertion counter {a_counter} > attestation counter", a_counter > counter))
print("ALL PASS" if all(results) else "SOME FAILED")
sys.exit(0 if all(results) else 1)
