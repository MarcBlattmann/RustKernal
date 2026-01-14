# Spacer

An invisible element that takes up remaining space in a layout.

## Syntax

```xml
<spacer/>
```

## Attributes

None - spacer automatically fills available space.

## Example

### Right-align buttons
```xml
<app title="Spacer Demo" width="300" height="100" x="100" y="100">
    <hbox gap="10" padding="20">
        <spacer/>
        <button width="80" height="30">OK</button>
        <button width="80" height="30">Cancel</button>
    </hbox>
</app>
```

Result:
```
+-----------------------------------------------+
|                           [  OK  ] [ Cancel ] |
+-----------------------------------------------+
```

### Center content
```xml
<hbox padding="10">
    <spacer/>
    <button width="100" height="30">Centered</button>
    <spacer/>
</hbox>
```

### Push footer to bottom
```xml
<vbox padding="10">
    <label>Header</label>
    <spacer/>
    <label>Footer (at bottom)</label>
</vbox>
```

## Notes

- Use in hbox to push items right or center them
- Use in vbox to push items down or center them
- Multiple spacers share remaining space equally
- See also: [vbox](vbox.md), [hbox](hbox.md)
