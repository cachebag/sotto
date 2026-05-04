_sotto_accepted=0
_sotto_highlight_entry=""
_sotto_active=0
_sotto_suggestion=""

# tell zsh-autosuggestions to ignore git commit commands (sotto owns this space)
ZSH_AUTOSUGGEST_HISTORY_IGNORE="${ZSH_AUTOSUGGEST_HISTORY_IGNORE:+$ZSH_AUTOSUGGEST_HISTORY_IGNORE|}git com*"

# remove only sotto's highlight entry without touching other plugins
_sotto_clear_highlight() {
    if [[ -n "$_sotto_highlight_entry" ]]; then
        region_highlight=("${region_highlight[@]:#$_sotto_highlight_entry}")
        _sotto_highlight_entry=""
    fi
}

# true when buffer is git commit -m, optional spaces only, or same then a quoted message
_sotto_in_commit_message() {
    [[ "$BUFFER" == git\ commit\ -m* ]] || return 1
    local rest="${BUFFER#git commit -m}"
    while [[ -n "$rest" && "$rest" == [[:space:]]* ]]; do
        rest="${rest#[[:space:]]}"
    done
    [[ -z "$rest" ]] && return 0
    [[ "$rest" == \'* || "$rest" == \"* ]] && return 0
    return 1
}

_sotto_ghost() {
    # only clean up if sotto was previously active (don't touch other plugins' POSTDISPLAY)
    if ! _sotto_in_commit_message; then
        if (( _sotto_active )); then
            POSTDISPLAY=""
            _sotto_clear_highlight
        fi
        _sotto_accepted=0
        _sotto_active=0
        _sotto_suggestion=""
        return
    fi

    # don't show ghost text after it's been accepted
    if (( _sotto_accepted )); then
        POSTDISPLAY=""
        _sotto_clear_highlight
        _sotto_active=0
        return
    fi

    # get sotto's suggestion
    _sotto_suggestion=$(sotto complete 2>/dev/null)

    if [[ -z "$_sotto_suggestion" ]]; then
        POSTDISPLAY=""
        _sotto_clear_highlight
        _sotto_active=0  # let autosuggestions work if sotto has nothing
        return
    fi

    _sotto_active=1  # only disable autosuggestions when sotto has a suggestion

    # include quotes in ghost if user hasn't typed one yet
    local rest="${BUFFER#git commit -m}"
    while [[ -n "$rest" && "$rest" == [[:space:]]* ]]; do
        rest="${rest#[[:space:]]}"
    done
    if [[ -z "$rest" ]]; then
        POSTDISPLAY="'${_sotto_suggestion}'"
    else
        POSTDISPLAY="$_sotto_suggestion"
    fi

    # style the ghost text grey — track our entry so we only remove ours
    _sotto_clear_highlight
    local start=${#BUFFER}
    local end=$(( start + ${#POSTDISPLAY} ))
    _sotto_highlight_entry="$start $end fg=green,italic"
    region_highlight+=("$_sotto_highlight_entry")
}

_sotto_do_accept() {
    local rest="${BUFFER#git commit -m}"
    while [[ -n "$rest" && "$rest" == [[:space:]]* ]]; do
        rest="${rest#[[:space:]]}"
    done
    local qc
    if [[ -z "$rest" ]]; then
        qc="'"
    elif [[ "$rest" == \'* || "$rest" == \"* ]]; then
        qc="${rest:0:1}"
    else
        return 1
    fi
    BUFFER="git commit -m ${qc}${_sotto_suggestion}${qc}"
    CURSOR=${#BUFFER}
    POSTDISPLAY=""
    _sotto_clear_highlight
    _sotto_accepted=1
    return 0
}

_sotto_accept() {
    if [[ -n "$_sotto_suggestion" ]] && _sotto_in_commit_message && _sotto_do_accept; then
        return
    fi
    zle expand-or-complete  # default Tab behavior
}

_sotto_forward_char() {
    if [[ -n "$_sotto_suggestion" ]] && _sotto_in_commit_message && _sotto_do_accept; then
        return
    fi
    zle forward-char  # default right arrow behavior
}

_sotto_end_of_line() {
    if [[ -n "$_sotto_suggestion" ]] && _sotto_in_commit_message && _sotto_do_accept; then
        return
    fi
    zle end-of-line  # default End key behavior
}

# register widgets
zle -N _sotto_ghost
zle -N _sotto_accept
zle -N _sotto_forward_char
zle -N _sotto_end_of_line

# hook into the line editor
autoload -Uz add-zle-hook-widget
add-zle-hook-widget line-pre-redraw _sotto_ghost

# bind keys to accept
bindkey '^I' _sotto_accept        # Tab
bindkey '^[[C' _sotto_forward_char # Right arrow
bindkey '^[OC' _sotto_forward_char # Right arrow (alternate)
bindkey '^E' _sotto_end_of_line    # Ctrl+E / End

# wrap zsh-autosuggestions to disable it when sotto is active
# this prevents async callbacks from overwriting sotto's POSTDISPLAY
if (( ${+functions[_zsh_autosuggest_suggest]} )); then
    functions[_sotto_orig_autosuggest_suggest]=$functions[_zsh_autosuggest_suggest]
    _zsh_autosuggest_suggest() {
        (( _sotto_active )) && return
        _sotto_orig_autosuggest_suggest "$@"
    }
fi
