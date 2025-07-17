# Interfaces

Parser to graph builder:

- Imports
- References
  - Functions
  - Classes
- Definitions
  - Functions
  - Classes

## Nodes

### Import

|Field|Type|Description|
|-----|----|-----------|
|path|str|Relative path of file performing the import|
|type|str|Either `import` or `from_import`|
|module|str|Name of module|
|alias|str|Import alias of module|
|line|int|Line number of import statement|

#### Example

```py
import math as ma
```

is equivalent to

```py
{
  "path": "example.py",
  "type": "import",
  "module": "math",
  "alias": "ma",
  "line": 1
}
```
