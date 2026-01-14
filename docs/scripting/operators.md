# Operators

PursuitScript supports arithmetic, comparison, and logical operators.

## Arithmetic Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `a + b` |
| `-` | Subtraction | `a - b` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `%` | Modulo (remainder) | `a % b` |

### String Concatenation

The `+` operator also concatenates strings:

```
var greeting = "Hello, " + name
```

## Comparison Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `==` | Equal | `a == b` |
| `!=` | Not equal | `a != b` |
| `<` | Less than | `a < b` |
| `>` | Greater than | `a > b` |
| `<=` | Less or equal | `a <= b` |
| `>=` | Greater or equal | `a >= b` |

## Logical Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `&&` | Logical AND | `a && b` |
| `\|\|` | Logical OR | `a \|\| b` |
| `!` | Logical NOT | `!a` |

## Unary Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `-` | Negation | `-value` |
| `!` | Logical NOT | `!enabled` |

## Operator Precedence

From highest to lowest:

1. `!`, `-` (unary)
2. `*`, `/`, `%`
3. `+`, `-`
4. `<`, `>`, `<=`, `>=`
5. `==`, `!=`
6. `&&`
7. `||`

Use parentheses to override precedence:

```
var result = (a + b) * c
var check = (x > 0) && (y < 10)
```

## Examples

```
// Arithmetic
var total = price * quantity
var average = sum / count
var isEven = number % 2 == 0

// Comparison
if score >= highScore {
    highScore = score
}

// Logical
if isLoggedIn && hasPermission {
    showContent()
}

if !enabled {
    message = "Disabled"
}

// Combined
if (age >= 18) && (hasID || withParent) {
    allowed = true
}
```
