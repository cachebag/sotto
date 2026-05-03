_sotto_ghost() {
    # only trigger when the buffer matches `git commit -m '...'`
    # we should consider other instances like `am`
    if [[ "$BUFFER" != git\ commit\ -m\ \'* && "$BUFFER" != git\ commit\ -m\ \"* ]]; then
        POSTDISPLAY=""
        return
    fi

    # extract what's already typed inside the quotes
    local typed="${BUFFER##git commit -m [\'\"]*}"

    # get sotto's suggestion
    local suggestion
    suggestion=$(sotto complete 2>/dev/null)

    if [[ -z "$suggestion" ]]; then
        POSTDISPLAY=""
        return
    fi

    # only show the part they haven't typed yet
    if [[ "$suggestion" == "$typed"* ]]; then
        POSTDISPLAY="${suggestion#$typed}"
    else
        POSTDISPLAY="$suggestion"
    fi
}

_sotto_accept() {
    if [[ -n "$POSTDISPLAY" ]]; then
        BUFFER="${BUFFER}${POSTDISPLAY}"
        CURSOR=${#BUFFER}
        POSTDISPLAY=""
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
