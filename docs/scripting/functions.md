# Functions

Functions are reusable blocks of code that can be called from event handlers.

## Syntax

```
func functionName(param1, param2) {
    // statements
    return value
}
```

## Basic Function

```
func sayHello() {
    message = "Hello!"
}
```

## With Parameters

```
func add(a, b) {
    return a + b
}

func greet(name) {
    message = "Hello, " + name
}
```

## With Return Value

```
func multiply(x, y) {
    return x * y
}

func isPositive(n) {
    if n > 0 {
        return true
    }
    return false
}
```

## Calling Functions

From `on_click`:
```xml
<button on_click="increment()">+1</button>
<button on_click="reset()">Reset</button>
```

From other functions:
```
func doubleIncrement() {
    increment()
    increment()
}
```

## Example

```xml
<app title="Calculator" width="200" height="180" x="100" y="100">
    <script>
        var result = 0
        var current = 0
        
        func addNumber(n) {
            current = current * 10 + n
        }
        
        func calculate() {
            result = current
            current = 0
        }
        
        func clear() {
            result = 0
            current = 0
        }
    </script>
    
    <vbox padding="10">
        <label>Result: {result}</label>
        <label>Input: {current}</label>
        <hbox gap="5">
            <button on_click="addNumber(1)">1</button>
            <button on_click="addNumber(2)">2</button>
            <button on_click="addNumber(3)">3</button>
        </hbox>
        <hbox gap="5">
            <button on_click="calculate()">=</button>
            <button on_click="clear()">C</button>
        </hbox>
    </vbox>
</app>
```

## Notes

- Functions must be defined in the `<script>` block
- Function names should be alphanumeric with underscores
- Parameters are passed by value
- Return is optional; functions return `null` by default
