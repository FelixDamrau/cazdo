Set Theme "Catppuccin Mocha"
Set FontSize 20
Set Width 1400
Set Height 900
Set Framerate 30
Set Padding 24
Set TypingSpeed 0.1s

Hide
Type "export PATH=.:$PATH"
Enter
Ctrl+L
Show

Type "cazdo"
Sleep 0.5s
Enter
Sleep 2s

# Hero still: work item 103 (Markdown description + HTML acceptance criteria).
Screenshot cazdo-open-nav-still.png
Sleep 0.5s

# Browse branches
Type "j"
Sleep 1.2s
Type "j"
Sleep 1.2s

# Reveal protected branches, walk the cursor onto the protected `main` (🔒), then hide
Type "p"
Sleep 1s
Type "j"
Sleep 0.5s
Type "j"
Sleep 0.5s
Type "j"
Sleep 2s
Type "p"
Sleep 1s

# Filter by a shared term, then clear it
Type "/"
Sleep 0.5s
Type "feature"
Sleep 1s
Enter
Sleep 1.5s
Escape
Sleep 1s

# Toggle to remote branches and back
Type "t"
Sleep 1.5s
Type "t"
Sleep 1.2s

Type "q"
Sleep 1s
