# Failures finding the sheprd-relay-agent build Sheprd installs on a machine
# it shares as a fleet's plain host.

# $path is a folder in this app's state; $error is the system's reason.
error-fleet-agent-build-cache = Could not make a folder for the relay agent's download in { $path }: { $error }
# $target is a Rust target such as aarch64-unknown-linux-musl; $error says why.
error-fleet-agent-build-download = Could not download this release's sheprd-relay-agent for { $target }: { $error }
# $platform is the host's platform, such as "Linux x86_64".
error-fleet-agent-build-no-build = This development build ships no sheprd-relay-agent for { $platform }
error-fleet-agent-build-no-state-dir = Could not find this app's state directory
# $platform is what the host reported, such as "FreeBSD amd64".
error-fleet-agent-build-unsupported = A plain host runs Linux or macOS on x86_64 or ARM64; this host reports { $platform }
