# Quick Start: Create Your First App

Get a working app running in 5 minutes.

## Option 1: Simple .pa File App (Easiest)

### Step 1: Create the file
Create `kernel/apps/hello.pa`:

```xml
<app title="Hello App" width="400" height="300">
  <vbox padding="20">
    <label text="Welcome to Pursuit OS!" />
    <label text="" />
    <button text="Say Hello" id="greet_btn" />
    <label text="" id="output" />
  </vbox>
  
  <script>
    var message = "Hello!";
    
    func on_greet_btn_click() {
      output.text = message;
    }
  </script>
</app>
```

### Step 2: Build & Run
```powershell
cd kernel
cargo run
```

In QEMU, type `ui` to start the GUI, then click Start menu to launch "Hello App".

### Step 3: Customize
Edit the XML to:
- Change `title` to your app name
- Add more buttons/labels
- Modify the script logic

---

## Option 2: Using the Code Editor

### Step 1: Start the GUI
```powershell
cargo run  # type 'ui' in the shell
```

### Step 2: Open Code Editor
- Click Start button → Code Editor
- File → New
- Type your app code

### Step 3: Save & Run
- File → Save (as `hello.pa`)
- The app loads automatically when saved

---

## Option 3: Native Rust App (Advanced)

### Step 1: Edit `builtin_apps.rs`
Add your app struct:

```rust
pub struct MyApp {
    counter: u32,
}

impl MyApp {
    pub fn new() -> Self {
        MyApp { counter: 0 }
    }
    
    pub fn render(&mut self, screen: &mut Screen, bounds: &Rect) {
        // Draw "Counter: 0" at top of app window
        let text = format!("Counter: {}", self.counter);
        draw_text(screen, bounds.x as usize, bounds.y as usize, &text, 0xFFFFFFFF);
    }
    
    pub fn handle_key(&mut self, key: char, _ctrl: bool) {
        match key {
            '+' => self.counter += 1,
            '-' => if self.counter > 0 { self.counter -= 1; },
            _ => {}
        }
    }
}
```

### Step 2: Register the app
In `app.rs`, add:

```rust
pub fn create_my_app() -> AppDef {
    AppDef::new("My App", 100, 100, 400, 300)
}
```

### Step 3: Add to start menu
In `start_menu.rs`, add to the menu items:

```rust
"my_app" => "My App",
```

### Step 4: Build
```powershell
cargo run
```

---

## What You Just Built

✅ A fully functional GUI application
✅ Event handling (button clicks, key presses)
✅ State management (variables)
✅ Custom rendering (for native apps)

## Next Steps

- **Learn about widgets**: [Widgets & Layout](04-widgets-layout.md)
- **Add more functionality**: [PursuitScript](05-pursuit-script.md)
- **Complex apps**: [Native Rust Apps](06-native-apps.md)
- **See examples**: [Examples](07-examples.md)

## Troubleshooting

**Q: My app doesn't appear in the Start menu**
A: Make sure you registered it in `start_menu.rs`

**Q: Keyboard input isn't working**
A: Ensure your window is focused (click on it first)

**Q: Syntax error in .pa file**
A: Check the XML is well-formed (all tags closed properly)

---

Great job! You've just created your first Pursuit OS app! 🎉
