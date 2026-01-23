# PursuitScript Reference

PursuitScript is a simple scripting language
for creating interactive Pursuit OS apps.

## Variables

Declare variables with var:

  var count = 0
  var name = "Hello"
  var enabled = true

## Assignments

Change variable values:

  count = count + 1
  name = "World"
  enabled = false

## Functions

Define reusable code:

  func increment() {
      count = count + 1
  }

  func add(a, b) {
      return a + b
  }

## Control Flow

### If Statements

  if count > 10 {
      count = 0
  }

  if enabled {
      doSomething()
  } else {
      doOther()
  }

### While Loops

  while count < 10 {
      count = count + 1
  }

## Operators

### Arithmetic
- + Addition
- - Subtraction
- * Multiplication
- / Division
- % Modulo

### Comparison
- == Equal
- != Not equal
- < Less than
- > Greater than
- <= Less or equal
- >= Greater or equal

### Logical
- && And
- || Or
- ! Not

## Built-in Functions

- close() - Close the window
- open("app") - Open another app
- minimize() - Minimize window
- print("msg") - Debug output
