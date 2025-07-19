# Interfaces

Parser to graph builder:

- Imports
- Function definitions
- Function references
- Container definitions (classes, structs, etc.)
- Container references

## Nodes

### Import

|Field|Type|Description|
|-----|----|-----------|
|module|`str`|Name of module importing from|
|name|`str`|Name of module being imported|
|alias|`str`|Import alias of module|
|level|`int`|Relative level of import, with 0 being absolute import|

#### Example

```py
from functools import partial as par
```

is equivalent to

```py
{
  "module": "functools",
  "name": "partial
  "alias": "par",
  "level": 0
}
```
