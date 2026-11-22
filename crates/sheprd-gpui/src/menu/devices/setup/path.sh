set -eu
line='case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac'
prepend() {
    file=$1
    links=0
    while [ -L "$file" ]; do
        links=$((links + 1))
        [ "$links" -le 40 ] || exit 1
        link=$(readlink "$file") || exit 1
        case "$link" in
            /*) file=$link ;;
            *) file=$(dirname "$file")/$link ;;
        esac
    done
    if [ -f "$file" ] && grep -Fqx "$line" "$file"; then return; fi
    if [ -e "$file" ] && [ ! -f "$file" ]; then
        printf 'Cannot configure PATH in %s\n' "$file" >&2
        exit 1
    fi
    work=$(mktemp "$(dirname "$file")/.sheprd-path.XXXXXX") || exit 1
    trap 'rm -f "$work"' EXIT
    trap 'exit 1' HUP INT TERM
    if [ -f "$file" ]; then cp -p "$file" "$work"; fi
    printf '%s\n' "$line" > "$work"
    if [ -f "$file" ]; then cat "$file" >> "$work"; fi
    mv -f "$work" "$file"
    trap - EXIT HUP INT TERM
}
shell=${SHELL:-/bin/sh}
case "${shell##*/}" in
    zsh) prepend "${ZDOTDIR:-$HOME}/.zshenv" ;;
    bash)
        prepend "$HOME/.bashrc"
        if [ -f "$HOME/.bash_profile" ]; then prepend "$HOME/.bash_profile"
        elif [ -f "$HOME/.bash_login" ]; then prepend "$HOME/.bash_login"
        else prepend "$HOME/.profile"; fi
        ;;
    fish)
        dir="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d"
        mkdir -p "$dir"
        file="$dir/sheprd-path.fish"
        line='fish_add_path --global "$HOME/.local/bin"'
        prepend "$file"
        ;;
    *) prepend "$HOME/.profile" ;;
esac
