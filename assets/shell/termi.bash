# Optional Termi Bash integration.
# Emits OSC 7 for the working directory and OSC 2 for the tab title.

__termi_percent_encode_path() {
    local value=${1-}
    local character encoded index output=""
    local LC_ALL=C

    for ((index = 0; index < ${#value}; index++)); do
        character=${value:index:1}
        case $character in
            [a-zA-Z0-9._~/-]) output+=$character ;;
            *)
                printf -v encoded '%%%02X' "'$character"
                output+=$encoded
                ;;
        esac
    done
    printf '%s' "$output"
}

__termi_safe_title() {
    local value=${1-}
    local character encoded index output=""
    local LC_ALL=C

    for ((index = 0; index < ${#value}; index++)); do
        character=${value:index:1}
        case $character in
            [[:print:]]) output+=$character ;;
            *)
                printf -v encoded '%%%02X' "'$character"
                output+=$encoded
                ;;
        esac
    done
    printf '%s' "$output"
}

__termi_update_context() {
    [[ ${TERM_PROGRAM-} == Termi ]] || return 0
    local display_path encoded_host encoded_path hostname title username
    hostname=${HOSTNAME:-localhost}
    username=${USER:-user}
    display_path=$PWD
    if [[ -n ${HOME:-} && $display_path == "$HOME" ]]; then
        display_path="~"
    elif [[ -n ${HOME:-} && $display_path == "$HOME/"* ]]; then
        display_path="~/${display_path#"$HOME/"}"
    fi

    encoded_host=$(__termi_percent_encode_path "$hostname")
    encoded_path=$(__termi_percent_encode_path "$PWD")
    title=$(__termi_safe_title "$username@${hostname%%.*}:$display_path")
    printf '\033]7;file://%s%s\007' "$encoded_host" "$encoded_path"
    printf '\033]2;%s\007' "$title"
}

if [[ ${TERM_PROGRAM-} == Termi ]]; then
    if declare -p PROMPT_COMMAND 2>/dev/null | grep -q '^declare -a'; then
        PROMPT_COMMAND=(__termi_update_context "${PROMPT_COMMAND[@]}")
    elif [[ ${PROMPT_COMMAND-} != *'__termi_update_context'* ]]; then
        PROMPT_COMMAND="__termi_update_context${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
    fi
fi
