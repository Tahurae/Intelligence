// 1. Monoidal Spaces
space NoteInput = Vector
space ContextTag = Vector
space EmbeddingSpace = Vector
space MemoryMatrix = Tensor

// 2. Morphisms (2-in-1-out monoidal association)
encode_text : NoteInput -> EmbeddingSpace
tag_context : ContextTag -> EmbeddingSpace
associate_memory : (EmbeddingSpace || EmbeddingSpace) -> MemoryMatrix

// 3. String Diagram Composition Architecture
flow main = (encode_text || tag_context) >> associate_memory >> ~associate_memory
