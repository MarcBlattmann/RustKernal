# App Architecture

Understand how Pursuit OS apps work under the hood.

## System Overview

```
┌─────────────────────────────────────────┐
│           Pursuit OS GUI                │
├─────────────────────────────────────────┤
│  Desktop Environment                    │
│  ├─ Window Manager                      │
│  ├─ Start Menu                          │
│  ├─ Taskbar                             │
│  └─ Mouse/Keyboard Input Router         │
├─────────────────────────────────────────┤
│  App Layer                              │
│  ├─ Declarative Apps (.pa files)        │
│  ├─ Native Rust Apps                    │
│  └─ App Definition System               │
├─────────────────────────────────────────┤
│  Graphics & Input                       │
│  ├─ Screen Driver                       │
│  ├─ Keyboard Driver                     │
│  ├─ Mouse Driver                        │
│  └─ Widget Rendering                    │
└─────────────────────────────────────────┘
```

## App Types

### 1. Declarative Apps (.pa Format)

**What it is**: XML-like syntax for building UIs without code

**Pros**:
- No programming required
- Fast to prototype
- Easy to understand
- Great for simple-to-medium apps

**Cons**:
- Limited to built-in widgets
- Scripts are interpreted (slower)
- Can't access low-level system features

**Typical Use Cases**:
- Settings dialogs
- Simple tools
- Information displays
- Games with simple mechanics

**Structure**:
```
.pa file
├─ Layout (vbox, hbox, etc)
│  ├─ Widgets (label, button, textbox)
│  └─ Styling & attributes
└─ Script section (optional)
   ├─ Variables
   └─ Event handlers
```

### 2. Native Rust Apps

**What it is**: Full-featured apps written directly in Rust

**Pros**:
- Maximum performance
- Full system access
- Custom rendering
- No limitations

**Cons**:
- Requires Rust knowledge
- Longer development time
- Must recompile OS

**Typical Use Cases**:
- Code editor
- File manager
- Terminal emulator
- Graphics programs
- System tools

**Structure**:
```
builtin_apps.rs
├─ Struct definition
├─ new() constructor
├─ render() for drawing
├─ handle_key() for input
└─ Other methods
```

## Lifecycle

### Declarative App (.pa)

```
1. User clicks app in Start Menu
   ↓
2. .pa file is parsed by pa_parser
   ↓
3. AppDef structure is created
   ↓
4. Window is created in Window Manager
   ↓
5. Widgets are rendered each frame
   ↓
6. User interactions trigger script functions
   ↓
7. Script modifies widget properties
   ↓
8. Widgets re-render with new values
```

### Native App (Rust)

```
1. User clicks app in Start Menu
   ↓
2. Rust struct is instantiated
   ↓
3. Window is created with NativeApp enum variant
   ↓
4. render() is called each frame
   ↓
5. User input calls handle_key() / handle_special_key()
   ↓
6. App modifies its internal state
   ↓
7. Next render() call uses new state
```

## Component Details

### Window Manager
- Manages all open windows
- Handles z-ordering (which window is on top)
- Routes mouse clicks to correct window
- Routes keyboard input to focused window
- Manages window resizing and moving

### Start Menu
- Lists all available apps
- Launches apps when clicked
- Can be toggled with Start button

### Desktop
- Renders background
- Renders taskbar
- Manages Start menu visibility
- Routes input to Window Manager

### Widget System
- Provides UI building blocks (buttons, labels, etc)
- Handles rendering of standard elements
- Uses rectangles for positioning
- Supports basic styling

### Input Router
```
Keyboard Input
    ↓
Windows::handle_keyboard_input()
    ↓
Topmost Window::handle_key()
    ↓
If Native App: App::handle_key()
    ↓
If Script App: Invoke script function
```

## Data Flow

### Rendering Pipeline

```
Frame Start
    ↓
Check for dirty rectangles
    ↓
Clear dirty areas
    ↓
Render windows in z-order
    ↓
Render taskbar
    ↓
Render start menu
    ↓
Show cursor
    ↓
Frame End
```

### Event Handling

```
Input Event (key press, mouse click)
    ↓
Desktop::handle_input() or handle_keyboard_input()
    ↓
Window Manager routes to correct window
    ↓
Window processes event
    ├─ For native apps: Call app method
    └─ For script apps: Invoke script function
    ↓
App updates its state
    ↓
Mark window as needing redraw (dirty rect)
    ↓
Next frame renders the change
```

## Memory Model

### Stack-based
- Window structs live in WindowManager
- App structs live in their Window
- Input processed immediately

### Heap-based
- Strings, Vectors (AppDef, widget lists)
- Script state
- File buffers

## Performance Characteristics

### Declarative Apps
- **Startup**: Moderate (parsing)
- **Runtime**: Good (native rendering)
- **Memory**: Low to Moderate
- **Input response**: Responsive

### Native Apps
- **Startup**: Fast (instant)
- **Runtime**: Excellent (native code)
- **Memory**: Depends on implementation
- **Input response**: Very responsive

## Threading Model

Pursuit OS runs **single-threaded**:
- All apps share the same execution context
- Events processed sequentially
- Simpler design, no race conditions
- Apps should not block for long periods

## File System Integration

Apps can access files via:

```rust
// Read file
let content = FILESYSTEM.lock().read_file("filename");

// Write file
FILESYSTEM.lock().write_file("filename", data);

// List directory
let files = FILESYSTEM.lock().list_directory("path");
```

## Graphics Model

### Coordinate System
- Origin (0,0) is top-left
- X increases rightward
- Y increases downward
- Units are pixels

### Colors
- 32-bit ARGB format
- `0xAARRGGBB`
- Example: `0xFF00FF00` is opaque green

### Rendering
- Direct pixel access to frame buffer
- No hardware acceleration
- All apps share single screen buffer
- Efficient dirty rectangle tracking

## Next Steps

- **Learn about .pa files**: [Declarative Apps (.pa format)](03-pa-format.md)
- **See it in action**: [Examples](07-examples.md)
- **Native apps**: [Native Rust Apps](06-native-apps.md)
- **Widget system**: [Widgets & Layout](04-widgets-layout.md)
