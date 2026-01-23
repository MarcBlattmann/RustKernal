# Pursuit OS App Development Guide

Welcome to the comprehensive guide for developing applications on Pursuit OS. This guide covers everything you need to know to create powerful GUI applications.

## Table of Contents

1. **[Quick Start](01-quick-start.md)** - Create your first app in 5 minutes
2. **[App Architecture](02-architecture.md)** - Understand the system design
3. **[Declarative Apps (.pa format)](03-pa-format.md)** - Create apps using the XML-like syntax
4. **[Widgets & Layout](04-widgets-layout.md)** - Build user interfaces
5. **[PursuitScript](05-pursuit-script.md)** - Add interactivity and logic
6. **[Native Rust Apps](06-native-apps.md)** - Advanced apps with Rust
7. **[Examples](07-examples.md)** - Real-world app examples
8. **[API Reference](08-api-reference.md)** - Complete API documentation

## Quick Overview

Pursuit OS provides **two ways** to build applications:

### Method 1: Declarative Apps (.pa files)
Perfect for simple to medium complexity apps:
```xml
<app title="My App" width="400" height="300">
  <vbox>
    <label text="Hello, World!" />
    <button text="Click Me" id="btn1" />
  </vbox>
  
  <script>
    func on_btn1_click() {
      print("Button clicked!");
    }
  </script>
</app>
```

### Method 2: Native Rust Apps
For complex, high-performance apps:
```rust
pub struct MyApp {
    data: String,
}

impl MyApp {
    pub fn new() -> Self {
        MyApp { data: String::new() }
    }
    
    pub fn render(&mut self, screen: &mut Screen, bounds: &Rect) {
        // Custom rendering
    }
}
```

## File Organization

Store your `.pa` files in:
```
kernel/apps/
├── myapp.pa
├── editor.pa
└── calculator.pa
```

Native apps are built into:
```
kernel/src/gui/builtin_apps.rs
```

## Next Steps

- **New to app development?** Start with [Quick Start](01-quick-start.md)
- **Want to understand the system?** Read [App Architecture](02-architecture.md)
- **Ready to build?** Jump to [Declarative Apps (.pa format)](03-pa-format.md)
- **Need inspiration?** Check [Examples](07-examples.md)

---

## System Requirements

- Understanding of basic GUI concepts (windows, buttons, text)
- For native apps: Familiarity with Rust
- For declarative apps: No programming experience needed!

## Support

If you encounter issues or have questions:
- Check [API Reference](08-api-reference.md) for detailed documentation
- Review [Examples](07-examples.md) for working code samples
- Check error messages in the GUI for hints

Happy building! 🚀
