# Textorium TUI Demo
# Records a GIF showing navigation, search, and pane switching

Output demo.gif

Set Shell zsh
Set FontSize 14
Set Width 1200
Set Height 700
Set Padding 10
Set Theme "Catppuccin Mocha"
Set TypingSpeed 80ms

# Set up PATH (hidden from recording)
Hide
Type "export PATH=$HOME/.cargo/bin:$PATH"
Enter
Sleep 500ms
Show

# Launch textorium
Type "textorium"
Enter
Sleep 3s

# Navigate down through posts
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 600ms

# Switch to metadata pane
Type "l"
Sleep 800ms

# Navigate metadata fields
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 600ms

# Switch to content preview pane
Type "l"
Sleep 800ms

# Scroll content preview
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Type "j"
Sleep 600ms

# Back to posts pane
Type "h"
Sleep 300ms
Type "h"
Sleep 600ms

# Search
Type "/"
Sleep 500ms
Type "rust"
Sleep 1s

# Clear search with Escape
Escape
Sleep 600ms

# Cycle sort mode
Type "s"
Sleep 600ms
Type "s"
Sleep 600ms

# Toggle draft filter
Type "f"
Sleep 800ms
Type "f"
Sleep 600ms

# Show help overlay
Type "?"
Sleep 2s

# Dismiss help
Type "?"
Sleep 800ms

# Quit
Type "q"
Sleep 500ms
