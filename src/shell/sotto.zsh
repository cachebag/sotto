_sotto_accepted=0

_sotto_ghost() {
    # reset accepted flag when user starts a new command
    if [[ "$BUFFER" != git\ commit\ -m\ \'* && "$BUFFER" != git\ commit\ -m\ \"* ]]; then
        POSTDISPLAY=""
        region_highlight=()
        _sotto_accepted=0
        return
    fi

    # don't show ghost text after it's been accepted
    if (( _sotto_accepted )); then
        POSTDISPLAY=""
        region_highlight=()
        return
    fi

    # clear zsh-autosuggestions so we take priority
    zle autosuggest-clear 2>/dev/null || true

    # get sotto's suggestion
    local suggestion
    suggestion=$(sotto complete 2>/dev/null)

    if [[ -z "$suggestion" ]]; then
        POSTDISPLAY=""
        region_highlight=()
        return
    fi

    POSTDISPLAY="$suggestion"

    # style the ghost text grey
    local start=${#BUFFER}
    local end=$(( start + ${#POSTDISPLAY} ))
    region_highlight=("$start $end fg=8")
}

_sotto_accept() {
    if [[ -n "$POSTDISPLAY" ]]; then
        # extract quote character and rebuild buffer cleanly
        local quote="${BUFFER##git commit -m }"
        local quote_char="${quote:0:1}"

        BUFFER="git commit -m ${quote_char}${POSTDISPLAY}${quote_char}"
        CURSOR=${#BUFFER}
        POSTDISPLAY=""
        region_highlight=()
        _sotto_accepted=1
    else
        zle expand-or-complete  # default Tab behavior
    fi
}

# register widgets
zle -N _sotto_ghost
zle -N _sotto_accept

# hook into the line editor
autoload -Uz add-zle-hook-widget
add-zle-hook-widget line-pre-redraw _sotto_ghost

# bind Tab to accept
bindkey '^I' _sotto_accept
