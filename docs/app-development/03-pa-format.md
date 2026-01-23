# Declarative Apps (.pa Format)

Build GUI apps using simple XML-like syntax. No programming required!

## Introduction

The `.pa` format (Pursuit Application) lets you define UIs declaratively. Think HTML, but for desktop apps.

**Key Features**:
- XML-like tag syntax
- Automatic layout management
- Event-driven scripting
- Simple and intuitive

## Basic Structure

```xml
<app title="App Title" width="400" height="300" x="100" y="100">
  <!-- Layout & Widgets -->
  <vbox>
    <label text="Hello" />
    <button text="Click" id="btn1" />
  </vbox>
  
  <!-- Optional scripting -->
  <script>
    var counter = 0;
    
    func on_btn1_click() {
      counter++;
    }
  </script>
</app>
```

## App Element

Root element that defines the window.

### Attributes
```xml
<app
  title="My Application"      <!-- Window title (required) -->
  width="400"                 <!-- Window width in pixels (required) -->
  height="300"                <!-- Window height in pixels (required) -->
  x="100"                     <!-- X position (optional, default: 100) -->
  y="100"                     <!-- Y position (optional, default: 100) -->
/>
```

### Example
```xml
<app title="Calculator" width="300" height="400">
  <!-- content -->
</app>
```

## Layout Containers

### VBox (Vertical Box)
Stacks children vertically (top to bottom).

```xml
<vbox padding="10" gap="5">
  <label text="Top" />
  <label text="Middle" />
  <label text="Bottom" />
</vbox>
```

**Attributes**:
- `padding`: Space around content (pixels)
- `gap`: Space between children (pixels)

### HBox (Horizontal Box)
Stacks children horizontally (left to right).

```xml
<hbox padding="10" gap="5">
  <button text="Left" />
  <button text="Center" />
  <button text="Right" />
</hbox>
```

**Attributes**:
- `padding`: Space around content
- `gap`: Space between children

### Panel
A container with a visible border.

```xml
<panel padding="15">
  <vbox>
    <label text="Content in panel" />
  </vbox>
</panel>
```

**Attributes**:
- `padding`: Space around content
- `color`: Border color (hex, like "FF0000")

## Widgets

### Label
Display text (read-only).

```xml
<label text="Hello, World!" id="status" />
```

**Attributes**:
- `text`: The text to display
- `id`: Optional identifier for scripting

### Button
Clickable button with text.

```xml
<button text="Click Me" id="my_btn" />
```

**Attributes**:
- `text`: Button label
- `id`: Used for event handling (required)

**Event**: `on_{id}_click()`

```xml
<script>
  func on_my_btn_click() {
    print("Button was clicked!");
  }
</script>
```

### TextBox
Text input field.

```xml
<textbox id="input" />
```

**Attributes**:
- `id`: For script access

**Script Access**:
```xml
<script>
  var user_input = "";
  
  func get_input() {
    return input.text;
  }
</script>
```

### Spacer
Empty space for layout.

```xml
<hbox>
  <button text="Left" />
  <spacer />           <!-- Pushes right button to the right -->
  <button text="Right" />
</hbox>
```

**Attributes**:
- `width`: Optional fixed width
- `height`: Optional fixed height

## Complete Example

```xml
<app title="To-Do List" width="400" height="500">
  <vbox padding="15" gap="10">
    <!-- Title -->
    <label text="My Tasks" id="title" />
    
    <!-- Input section -->
    <hbox gap="5">
      <textbox id="task_input" />
      <button text="Add" id="add_btn" />
    </hbox>
    
    <!-- Task list display -->
    <panel padding="10">
      <label text="Tasks:" />
      <label text="" id="task_list" />
    </panel>
    
    <!-- Stats -->
    <label text="Total: 0" id="stats" />
  </vbox>
  
  <script>
    var tasks = "";
    var count = 0;
    
    func on_add_btn_click() {
      var task = task_input.text;
      if (task != "") {
        tasks = tasks + task + "\n";
        count = count + 1;
        task_list.text = tasks;
        stats.text = "Total: " + count;
        task_input.text = "";
      }
    }
  </script>
</app>
```

## Styling (Future)

Basic styling is supported:

```xml
<button text="Styled" color="FF0000" font_size="14" />
```

**Common Attributes** (when available):
- `color`: Text color (hex, format: "RRGGBB")
- `background`: Background color
- `font_size`: Text size in pixels

## Tips & Best Practices

### 1. Use IDs for Event Handlers
```xml
<!-- Good: Has ID for event handler -->
<button text="Send" id="send_btn" />

<!-- Bad: No ID, event can't be triggered -->
<button text="Send" />
```

### 2. Nest Containers for Complex Layouts
```xml
<vbox>
  <hbox>
    <button text="File" />
    <button text="Edit" />
  </hbox>
  <panel>
    <!-- Main content -->
  </panel>
  <hbox>
    <label text="Status" />
  </hbox>
</vbox>
```

### 3. Use Spacers for Alignment
```xml
<!-- Center a button -->
<hbox>
  <spacer />
  <button text="Center" />
  <spacer />
</hbox>
```

### 4. Keep Scripts Simple
Complex logic belongs in native apps. Keep .pa scripts for simple interactions.

### 5. Use Clear IDs
```xml
<!-- Good IDs -->
<button text="Submit" id="submit_btn" />
<textbox id="email_input" />
<label text="" id="error_message" />

<!-- Confusing IDs -->
<button text="Go" id="b1" />
<textbox id="t" />
```

## Common Patterns

### Form
```xml
<vbox padding="20" gap="10">
  <label text="Login Form" />
  <textbox id="username" />
  <textbox id="password" />
  <button text="Sign In" id="signin_btn" />
</vbox>

<script>
  func on_signin_btn_click() {
    var user = username.text;
    var pass = password.text;
    print(user);
  }
</script>
```

### Menu
```xml
<vbox gap="5" padding="10">
  <button text="New" id="menu_new" />
  <button text="Open" id="menu_open" />
  <button text="Save" id="menu_save" />
  <button text="Exit" id="menu_exit" />
</vbox>

<script>
  func on_menu_new_click() {
    print("Creating new file...");
  }
  
  func on_menu_open_click() {
    print("Opening file...");
  }
</script>
```

### Settings Panel
```xml
<app title="Settings" width="350" height="300">
  <vbox padding="15" gap="15">
    <label text="Settings" />
    
    <hbox gap="10">
      <label text="Volume:" />
      <textbox id="volume" />
    </hbox>
    
    <hbox gap="10">
      <label text="Theme:" />
      <textbox id="theme" />
    </hbox>
    
    <hbox gap="5">
      <button text="Save" id="save_btn" />
      <button text="Cancel" id="cancel_btn" />
    </hbox>
  </vbox>
  
  <script>
    func on_save_btn_click() {
      print("Settings saved!");
    }
  </script>
</app>
```

## Limitations

1. **Widgets**: Only basic widgets (label, button, textbox) supported
2. **Performance**: Interpreted, slower than native apps
3. **Graphics**: No custom drawing, use native apps for that
4. **System Access**: Limited file system access

For advanced apps, use [Native Rust Apps](06-native-apps.md).

## Next Steps

- **Add scripting**: [PursuitScript](05-pursuit-script.md)
- **See examples**: [Examples](07-examples.md)
- **Go native**: [Native Rust Apps](06-native-apps.md)
- **Layout guide**: [Widgets & Layout](04-widgets-layout.md)
