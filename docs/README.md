# Pursuit App (.pa) Documentation

Documentation for the `.pa` file format used to create GUI applications in Pursuit OS.

## Quick Start

1. Create a `.pa` file in `kernel/apps/`
2. Build with `cargo run`
3. Your app appears in the Start Menu!

## Example

```xml
<app title="Hello" width="200" height="150" x="100" y="100">
    <label x="20" y="30">Hello World!</label>
    <button x="50" y="80" width="100" height="30">OK</button>
</app>
```