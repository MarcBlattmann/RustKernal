# Events

Events allow your app to respond to user interactions.

## on_click

Triggered when a button is clicked.

### Syntax

```xml
<button on_click="handler">Label</button>
```

### Inline Statements

Execute simple statements directly:

```xml
<button on_click="count = count + 1">+1</button>
<button on_click="close()">Close</button>
<button on_click="enabled = !enabled">Toggle</button>
```

### Function Calls

Call a function defined in `<script>`:

```xml
<button on_click="increment()">Increment</button>
<button on_click="saveSettings()">Save</button>
<button on_click="addNumber(5)">Add 5</button>
```

### Multiple Statements

Separate statements with semicolons:

```xml
<button on_click="count = 0; message = 'Reset!'">Reset All</button>
```

## Text Interpolation

Labels automatically update when variables change.

### Syntax

Use `{variableName}` in label text:

```xml
<label>Count: {count}</label>
<label>Hello, {username}!</label>
<label>{status}</label>
```

### Example

```xml
<app title="Live Update" width="200" height="150" x="100" y="100">
    <script>
        var count = 0
        var status = "Ready"
    </script>
    
    <vbox padding="10">
        <label>Count: {count}</label>
        <label>Status: {status}</label>
        <button on_click="count = count + 1; status = 'Clicked!'">
            Click Me
        </button>
    </vbox>
</app>
```

## Complete Example

```xml
<app title="Toggle Demo" width="250" height="200" x="100" y="100">
    <script>
        var darkMode = false
        var volume = 50
        var muted = false
        
        func toggleDark() {
            darkMode = !darkMode
        }
        
        func toggleMute() {
            muted = !muted
        }
        
        func adjustVolume(delta) {
            if !muted {
                volume = volume + delta
                if volume < 0 {
                    volume = 0
                }
                if volume > 100 {
                    volume = 100
                }
            }
        }
    </script>
    
    <vbox padding="10" gap="8">
        <hbox>
            <label>Dark Mode:</label>
            <spacer/>
            <button on_click="toggleDark()">Toggle</button>
        </hbox>
        
        <hbox>
            <label>Volume: {volume}</label>
            <spacer/>
            <button on_click="adjustVolume(-10)">-</button>
            <button on_click="adjustVolume(10)">+</button>
        </hbox>
        
        <hbox>
            <label>Muted:</label>
            <spacer/>
            <button on_click="toggleMute()">Toggle</button>
        </hbox>
        
        <spacer/>
        
        <button on_click="close()">Done</button>
    </vbox>
</app>
```

## Future Events

These events may be added in future versions:

- `on_change` - TextBox value changed
- `on_focus` - Element gained focus
- `on_blur` - Element lost focus
- `on_key` - Key pressed
