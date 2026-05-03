function _sotto_suggest
    set -l cmd (commandline)

    # only trigger on `git commit -m`
    # consider `am` as well
    if not string match -q 'git commit -m *' -- $cmd
        return
    end

    # get sotto's suggestion
    set -l suggestion (sotto complete 2>/dev/null)

    if test -n "$suggestion"
        commandline -f repaint
        echo -n $suggestion
    end
end

function _sotto_ghost --on-event fish_prompt
    # register our custom autosuggestion
    bind \t '_sotto_accept'
end

function _sotto_accept
    set -l cmd (commandline)

    if string match -q 'git commit -m *' -- $cmd
        set -l suggestion (sotto complete 2>/dev/null)
        if test -n "$suggestion"
            commandline -r "git commit -m '$suggestion'"
            commandline -f end-of-line
            return
        end
    end

    commandline -f complete  # default Tab
end
