# PursuitScript Documentation

PursuitScript is the scripting language for `.pa` app files. It allows you to add interactivity to your apps with variables, functions, and event handlers.

## Quick Start

```xml
<app title="Counter" width="200" height="150" x="100" y="100">
    <script>
        var count = 0
        
        func increment() {
            count = count + 1
        }
    </script>
    
    <vbox padding="10">
        <label>Count: {count}</label>
        <button on_click="increment()">+1</button>
        <button on_click="close()">Close</button>
    </vbox>
</app>
```

## Documentation Structure

```
scripting/
├── README.md          # This file
├── variables.md       # Variables and data types
├── functions.md       # User-defined functions
├── control-flow.md    # If statements, while loops
├── operators.md       # All operators
├── built-in.md        # Built-in functions
└── events.md          # Event handlers (on_click, etc.)
```

## Features

- **Variables**: Store and manipulate data
- **Functions**: Reusable code blocks
- **Control Flow**: `if`/`else`, `while` loops
- **Text Interpolation**: Display variables with `{varname}`
- **Event Handlers**: Respond to user interactions
- **Built-in Functions**: `close()`, `open()`, `minimize()`
