# Pursuit App (.pa) File Format

The `.pa` file format is a simple XML-like markup for defining GUI applications in Pursuit OS.

## Basic Structure

```xml
<app title="My App" width="300" height="200" x="100" y="100">
    <!-- UI elements go here -->
</app>
```

### App Attributes

| Attribute | Required | Description |
|-----------|----------|-------------|
| `title`   | Yes      | Window title bar text |
| `width`   | Yes      | Window width in pixels |
| `height`  | Yes      | Window height in pixels |
| `x`       | Yes      | Initial X position on screen |
| `y`       | Yes      | Initial Y position on screen |

## UI Elements

### Label
Displays static text.

```xml
<label x="20" y="30">Your text here</label>
```

| Attribute | Description |
|-----------|-------------|
| `x`       | X offset from content area |
| `y`       | Y offset from content area |

### Button
A clickable button with text.

```xml
<button x="20" y="100" width="80" height="30">Click Me</button>
```

| Attribute | Description |
|-----------|-------------|
| `x`       | X offset from content area |
| `y`       | Y offset from content area |
| `width`   | Button width in pixels |
| `height`  | Button height in pixels |

### Panel
A decorative panel/container (drawn as a bordered rectangle).

```xml
<panel x="10" y="50" width="280" height="100"/>
```

| Attribute | Description |
|-----------|-------------|
| `x`       | X offset from content area |
| `y`       | Y offset from content area |
| `width`   | Panel width in pixels |
| `height`  | Panel height in pixels |

### TextBox
An input field area (visual only for now).

```xml
<textbox x="10" y="30" width="180" height="30"/>
```

| Attribute | Description |
|-----------|-------------|
| `x`       | X offset from content area |
| `y`       | Y offset from content area |
| `width`   | TextBox width in pixels |
| `height`  | TextBox height in pixels |

## Complete Example

```xml
<app title="Calculator" width="200" height="280" x="500" y="100">
    <textbox x="10" y="30" width="180" height="30"/>
    
    <button x="10" y="70" width="40" height="35">7</button>
    <button x="55" y="70" width="40" height="35">8</button>
    <button x="100" y="70" width="40" height="35">9</button>
    <button x="145" y="70" width="45" height="35">/</button>
    
    <button x="10" y="110" width="40" height="35">4</button>
    <button x="55" y="110" width="40" height="35">5</button>
    <button x="100" y="110" width="40" height="35">6</button>
    <button x="145" y="110" width="45" height="35">*</button>
    
    <label x="10" y="200">Result: 0</label>
</app>
```

## Adding New Apps

1. Create a new `.pa` file in `kernel/apps/`
2. The filename (without extension) becomes the app ID
3. Rebuild with `cargo run`
4. The app automatically appears in the Start Menu!

## Naming Convention

- Filename: `my_app.pa` → App ID: `my_app` → Menu shows: "My App"
- Underscores and hyphens become spaces
- First letter of each word is capitalized

## Notes

- All coordinates are relative to the window's content area (below title bar)
- Elements scale proportionally when the window is resized
- Elements are clipped if they exceed the window bounds
- Self-closing tags (`<element/>`) and paired tags (`<element></element>`) both work
