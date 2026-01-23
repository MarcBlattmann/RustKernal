# Built-in Functions

PursuitScript provides built-in functions for common operations.

## Window Functions

### close()

Closes the current window.

```xml
<button on_click="close()">Close</button> 
```

### open(app_id)

Opens another app by its ID (filename without `.pa`).

```xml
<button on_click="open('calculator')">Calculator</button>
<button on_click="open('settings')">Settings</button>
```

## Filesystem Functions

### listDrives()

Returns an array of available drive names.

```
var drives = listDrives()
// drives = ["disk0", "disk1"]
```

### listFiles(drive)

Returns an array of files and directories on the specified drive.
Directories have a trailing `/`.

```
var files = listFiles("disk0")
// files = ["Documents/", "redame.txt", "data.bin"]
```

### readFile(drive, filename)

Reads and returns the content of a text file.

```
var content = readFile("disk0", "readme.txt")
```

### writeFile(drive, filename, content)

Writes content to a file. Returns `true` on success.

```
var success = writeFile("disk0", "notes.txt", "Hello World!")
```

### createFile(drive, filename)

Creates an empty file. Returns `true` on success.

```
var success = createFile("disk0", "newfile.txt")
```

### createDir(drive, dirname)

Creates a directory. Returns `true` on success.

```
var success = createDir("disk0", "MyFolder")
```

### deleteFile(drive, filename)

Deletes a file or directory. Returns `true` on success.

```
var success = deleteFile("disk0", "oldfile.txt")
```

### fileExists(drive, filename)

Returns `true` if the file or directory exists.

```
if fileExists("disk0", "config.txt") {
    var cfg = readFile("disk0", "config.txt")
}
```

### fileSize(drive, filename)

Returns the size of a file in bytes.

```
var size = fileSize("disk0", "data.bin")
```

### isDir(drive, name)

Returns `true` if the entry is a directory.

```
if isDir("disk0", "Documents") {
    // It's a folder
}
```

## Array Functions

### arrayLen(array)

Returns the number of elements in an array.

```
var files = listFiles("disk0")
var count = arrayLen(files)
```

### arrayGet(array, index)

Returns the element at the specified index (0-based).

```
var files = listFiles("disk0")
var first = arrayGet(files, 0)
```

## String Functions

### concat(str1, str2, ...)

Concatenates multiple values into a single string.

```
var msg = concat("Found ", count, " files")
```

## Math Functions

### abs(value)

Returns the absolute value.

```
var distance = abs(x - target)
```

### min(a, b)

Returns the smaller of two values.

```
var smallest = min(score1, score2)
```

### max(a, b)

Returns the larger of two values.

```
var largest = max(score1, score2)
var clamped = min(max(value, 0), 100)  // Clamp 0-100
```

## Example: File Explorer

```xml
<app title="File Explorer" width="400" height="300" x="100" y="100">
    <script>
        var currentDrive = "disk0"
        var files = listFiles(currentDrive)
        var fileCount = arrayLen(files)
        var selected = ""
        
        func refresh() {
            files = listFiles(currentDrive)
            fileCount = arrayLen(files)
        }
        
        func createNewFile() {
            createFile(currentDrive, "untitled.txt")
            refresh()
        }
        
        func deleteSelected() {
            if selected != "" {
                deleteFile(currentDrive, selected)
                selected = ""
                refresh()
            }
        }
    </script>
    
    <vbox padding="10">
        <label>Drive: {currentDrive} ({fileCount} items)</label>
        
        <hbox gap="5">
            <button on_click="createNewFile()">New File</button>
            <button on_click="deleteSelected()">Delete</button>
            <button on_click="refresh()">Refresh</button>
        </hbox>
        
        <spacer/>
        <button on_click="close()">Close</button>
    </vbox>
</app>
```

## Notes

- `open()` requires the app ID as a string in quotes
- Window functions affect only the current window
- Math functions work with integers
- Filesystem functions require a drive name (e.g., "disk0")
- Arrays are returned by `listFiles()` and `listDrives()`
