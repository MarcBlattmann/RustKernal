# Creating Apps

Learn to create Pursuit Apps (.pa files)
for Pursuit OS.

## App Structure

Every .pa file starts with <app>:

  <app title="My App" 
       width="300" 
       height="200"
       x="100" 
       y="100">
    
    <!-- Script section -->
    <script>
      var myVar = 0
      func myFunc() { }
    </script>
    
    <!-- UI elements -->
    <vbox>
      <label>Content here</label>
    </vbox>
    
  </app>

## App Attributes

- title - Window title
- width - Window width (pixels)
- height - Window height (pixels)
- x, y - Initial position

## Script Section

Add logic in <script> tags:

  <script>
    var counter = 0
    
    func increment() {
      counter = counter + 1
    }
    
    func reset() {
      counter = 0
    }
  </script>

## Dynamic Content

Use {variable} in labels:

  <label>Value: {counter}</label>

The label updates automatically
when the variable changes.

## Event Handlers

Connect buttons to functions:

  <button on_click="increment()">
    Add One
  </button>
  
  <button on_click="close()">
    Exit
  </button>

## File Location

Save .pa files in:
  /apps/

They appear in the Start Menu
automatically!

## Example: Counter App

  <app title="Counter" width="200">
    <script>
      var n = 0
      func add() { n = n + 1 }
      func sub() { n = n - 1 }
    </script>
    
    <vbox padding="20">
      <label>Count: {n}</label>
      <hbox gap="10">
        <button on_click="sub()">-</button>
        <button on_click="add()">+</button>
      </hbox>
    </vbox>
  </app>
