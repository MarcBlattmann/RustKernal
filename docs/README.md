# Pursuit App (.pa) Documentation

Documentation for the `.pa` file format used to create GUI applications in Pursuit OS.

## Quick Start

1. Create a `.pa` file in `kernel/apps/`
2. Build with `cargo run`
3. Your app appears in the Start Menu!

## File Structure

```
docs/
├── README.md              # This file
├── app.md                 # App container
├── elements/              # Basic UI elements
│   ├── label.md
│   ├── button.md
│   ├── panel.md
│   └── textbox.md
├── layout/                # Layout containers
│   ├── vbox.md
│   ├── hbox.md
│   └── spacer.md
└── scripting/             # PursuitScript
    ├── README.md
    ├── variables.md
    ├── functions.md
    ├── control-flow.md
    ├── operators.md
    ├── built-in.md
    └── events.md
```

## Basic Example

```xml
<app title="Hello" width="200" height="150" x="100" y="100">
    <label x="20" y="30">Hello World!</label>
    <button x="50" y="80" width="100" height="30">OK</button>
</app>
```

## Interactive Example

```xml
<app title="Counter" width="200" height="150" x="100" y="100">
    <script>
        var count = 0
    </script>
    
    <vbox padding="10">
        <label>Count: {count}</label>
        <button on_click="count = count + 1">+1</button>
        <button on_click="close()">Close</button>
    </vbox>
</app>
```