# UI Elements

Pursuit Apps use XML-like elements
to define the user interface.

## Label

Display text:

  <label>Hello World!</label>
  <label x="20" y="40">At position</label>

Use {variables} for dynamic text:

  <label>Count: {count}</label>

## Button

Clickable buttons:

  <button>Click Me</button>
  <button on_click="doSomething()">
    Action
  </button>

Attributes:
- width, height - Size
- on_click - Handler function

## TextBox

Text input field:

  <textbox width="200" height="25"/>

## Panel

Container with background:

  <panel width="300" height="200">
    <!-- children -->
  </panel>

## Common Attributes

All elements support:
- x, y - Position (pixels)
- width, height - Size (pixels)

## Example App

  <app title="My App" width="300">
    <script>
      var clicks = 0
      func clicked() {
        clicks = clicks + 1
      }
    </script>
    
    <vbox padding="10">
      <label>Clicks: {clicks}</label>
      <button on_click="clicked()">
        Click Me
      </button>
    </vbox>
  </app>
