# Variables

Variables store data that can be used and modified during app execution.

## Declaration

Use `var` to declare a variable:

```
var count = 0
var name = "Hello"
var enabled = true
```

## Data Types

| Type | Example | Description |
|------|---------|-------------|
| `Int` | `42`, `-10`, `0` | Integer numbers |
| `Float` | `3.14`, `-0.5` | Decimal numbers |
| `Bool` | `true`, `false` | Boolean values |
| `String` | `"Hello"`, `'World'` | Text strings |
| `Null` | `null` | No value |

## Assignment

Change a variable's value:

```
count = count + 1
name = "World"
enabled = false
```

## Text Interpolation

Display variables in labels using `{varname}`:

```xml
<label>Count: {count}</label>
<label>Hello, {name}!</label>
```

## Example

```xml
<app title="Variables Demo" width="250" height="200" x="100" y="100">
    <script>
        var clicks = 0
        var message = "Click the button!"
    </script>
    
    <vbox padding="10">
        <label>{message}</label>
        <label>Clicks: {clicks}</label>
        <button on_click="clicks = clicks + 1">Click Me</button>
    </vbox>
</app>
```

## Scope

- Variables declared in `<script>` are global to the window
- Function parameters are local to that function
- Variables persist for the lifetime of the window
