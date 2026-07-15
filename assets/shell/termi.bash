# Optional Termi Bash integration.
# Emits OSC 7 for the working directory and OSC 2 for the tab title.

__termi_percent_encode_path() {
    local value=${1-}
    value=${value//'%'/'%25'}
    value=${value//' '/'%20'}
    value=${value//'#'/'%23'}
    value=${value//'?'/'%3F'}
    printf '%s' "$value"
}

__termi_update_context() {
    [[ ${TERM_PROGRAM-} == Termi ]] || return 0
    local encoded_path
    encoded_path=$(__termi_percent_encode_path "$PWD")
    printf '\033]7;file://%s%s\007' "${HOSTNAME:-localhost}" "$encoded_path"
    printf '\033]2;%s@%s:%s\007' "${USER:-user}" "${HOSTNAME%%.*}" "${PWD/#$HOME/~}"
}

if [[ ${TERM_PROGRAM-} == Termi ]]; then
    if declare -p PROMPT_COMMAND 2>/dev/null | grep -q '^declare -a'; then
        PROMPT_COMMAND=(__termi_update_context "${PROMPT_COMMAND[@]}")
    elif [[ ${PROMPT_COMMAND-} != *'__termi_update_context'* ]]; then
        PROMPT_COMMAND="__termi_update_context${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
    fi
fi
