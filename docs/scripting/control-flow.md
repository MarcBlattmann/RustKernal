# Control Flow

Control flow statements let you execute code conditionally or repeatedly.

## If Statement

Execute code only when a condition is true:

```
if condition {
    // statements
}
```

### With Else

```
if count > 10 {
    message = "High"
} else {
    message = "Low"
}
```

### Nested If

```
if score >= 90 {
    grade = "A"
} else {
    if score >= 80 {
        grade = "B"
    } else {
        grade = "C"
    }
}
```

## While Loop

Repeat code while a condition is true:

```
while count < 10 {
    count = count + 1
}
```

### Safety Limit

Loops are limited to 10,000 iterations to prevent infinite loops.

## Conditions

Any expression can be used as a condition:

| Value | Truthy? |
|-------|---------|
| `true` | Yes |
| `false` | No |
| `0` | No |
| Non-zero number | Yes |
| `""` (empty string) | No |
| Non-empty string | Yes |
| `null` | No |

## Example

```xml
<app title="Game" width="250" height="200" x="100" y="100">
    <script>
        var score = 0
        var status = "Playing"
        
        func addPoints(points) {
            if status == "Playing" {
                score = score + points
                
                if score >= 100 {
                    status = "You Win!"
                }
            }
        }
        
        func reset() {
            score = 0
            status = "Playing"
        }
    </script>
    
    <vbox padding="10">
        <label>Score: {score}</label>
        <label>Status: {status}</label>
        <hbox gap="5">
            <button on_click="addPoints(10)">+10</button>
            <button on_click="addPoints(25)">+25</button>
        </hbox>
        <button on_click="reset()">New Game</button>
    </vbox>
</app>
```

## Best Practices

- Keep conditions simple and readable
- Avoid deep nesting (use functions instead)
- Be careful with while loops to avoid infinite loops
- Use meaningful variable names in conditions
