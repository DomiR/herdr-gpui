set -eu
install_key=$1
target=$2
remote_command=$3
skip_host_key=$4
shift 4
if [ "$install_key" = yes ]; then ssh-copy-id "$target"; fi
if [ "$skip_host_key" = yes ]; then
    ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o GlobalKnownHostsFile=/dev/null -o ConnectTimeout=15 -- "$target" "$remote_command"
else
    ssh -o StrictHostKeyChecking=yes -o ConnectTimeout=15 -- "$target" "$remote_command"
fi
exec "$@"
