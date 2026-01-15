# Built-in Functions

PursuitScript provides built-in functions for common operations.

## Window Functions

### close()

Closes the current window.

```xml
<button on_click="close()">Close</button> 
```

### open(app_id)

Opens another app by its ID (filename without `.pa`).

```xml
<button on_click="open('calculator')">Calculator</button>
<button on_click="open('settings')">Settings</button>
```
## Math Functions

### abs(value)

Returns the absolute value.

```
var distance = abs(x - target)
```

### min(a, b)

Returns the smaller of two values.

```
var smallest = min(score1, score2)
```

### max(a, b)

Returns the larger of two values.

```
var largest = max(score1, score2)
var clamped = min(max(value, 0), 100)  // Clamp 0-100
```

## Example

```xml
<app title="Multi-App Launcher" width="200" height="200" x="100" y="100">
    <script>
        var value = 50
        
        func clamp(v, minVal, maxVal) {
            return min(max(v, minVal), maxVal)
        }
        
        func adjust(delta) {
            value = clamp(value + delta, 0, 100)
        }
    </script>
    
    <vbox padding="10">
        <label>Value: {value}</label>
        
        <hbox gap="5">
            <button on_click="adjust(-10)">-10</button>
            <button on_click="adjust(10)">+10</button>
        </hbox>
        
        <spacer/>
        
        <label>Open Apps:</label>
        <button on_click="open('calculator')">Calculator</button>
        <button on_click="open('notepad')">Notepad</button>
        
        <spacer/>
        
        <button on_click="close()">Close</button>
    </vbox>
</app>
```

## Notes

- `open()` requires the app ID as a string in quotes
- Window functions affect only the current window
- Math functions work with integers
