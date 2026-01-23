# Layout System

Pursuit OS provides flexible layout
containers for organizing UI elements.

## VBox (Vertical Box)

Stack elements vertically:

  <vbox padding="10" gap="5">
    <label>First</label>
    <label>Second</label>
    <label>Third</label>
  </vbox>

Attributes:
- padding - Space around content
- gap - Space between items

## HBox (Horizontal Box)

Stack elements horizontally:

  <hbox padding="10" gap="5">
    <button>Left</button>
    <button>Right</button>
  </hbox>

## Spacer

Takes up remaining space:

  <vbox>
    <label>Top</label>
    <spacer/>
    <label>Bottom</label>
  </vbox>

Use spacers to push content:

  <hbox>
    <spacer/>
    <button>Right-aligned</button>
  </hbox>

## Nesting Layouts

Combine VBox and HBox:

  <vbox padding="10">
    <label>Title</label>
    <spacer/>
    <hbox>
      <spacer/>
      <button>Cancel</button>
      <button>OK</button>
    </hbox>
  </vbox>

## Tips

- Use VBox for forms
- Use HBox for button rows
- Use Spacer for alignment
- Nest layouts for complex UIs
