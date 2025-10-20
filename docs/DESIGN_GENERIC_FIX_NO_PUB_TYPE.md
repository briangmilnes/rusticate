# Design: Generic fix-no-pub-type Tool

## Current Status: InsertionSortSt Prototype

The current `rusticate-fix-no-pub-type` is a working prototype hardcoded for the InsertionSortSt pattern.

It successfully performs:
- ✅ Add `pub type T<S> = [S];`
- ✅ Transform trait signature: `fn insSort(&self, slice: &mut [T])` → `fn insSort(&mut self)`
- ✅ Transform impl header: `impl<T> Trait<T> for T` → `impl<S> Trait<S> for [S]`
- ✅ Transform method body: replace `slice` with `self`
- ✅ Transform call sites: `x.insSort(&mut data)` → `data.insSort()`
- ✅ Compile and test cleanly

## What a Generic Version Needs

### Phase 1: Enhanced Analysis

The analysis phase needs to extract from AST:

1. **Trait Information**
   - Trait name (e.g., `InsertionSortStTrait`)
   - Generic parameters and their bounds
   - Method names with unused `self` parameter
   - Actual data parameter for each method (name and type)

2. **Impl Information**
   - Current impl header pattern: `impl<T: Bounds> Trait<T> for T`
   - Generic parameter names (e.g., `T`)
   - Type being implemented for (e.g., `T` or a concrete type)
   - Actual data type from method parameters (e.g., `&mut [T]`)

3. **Method Information (per method with unused self)**
   - Method name (e.g., `insSort`)
   - Current signature: `fn method(&self, param: Type)`
   - Data parameter name (e.g., `slice`)
   - Data parameter type (e.g., `&mut [T]`)
   - Whether body uses the data parameter

### Phase 2: Transformation Planning

Based on analysis, determine:

1. **Pub Type to Add**
   - Extract the actual type from method parameters
   - Keep lifetimes and generics intact
   - Example: `pub type T<'a, S> = &'a mut [S]` or `pub type T<S> = [S]`

2. **Impl Header Transformation**
   - Source: `impl<T: Bounds> Trait<T> for T`
   - Determine target type from data parameter
   - Rename generic if there's a conflict
   - Target: `impl<S: Bounds> Trait<S> for [S]`

3. **Method Signature Transformation**
   - Source: `fn method(&self, data: &mut [T])`
   - Target: `fn method(&mut self)` or `fn method(&self)` based on mutability
   - Store mapping: old_param_name → `self`

4. **Method Body Transformation**
   - Replace all uses of old param name with `self`
   - Update type references if generic renamed

5. **Call Site Transformation**
   - Find: `receiver.method(&mut arg)` or `receiver.method(&arg)`
   - Replace: `arg.method()`
   - Handle all method names from analysis

### Phase 3: Implementation Strategy

```rust
struct UnusedSelfAnalysis {
    trait_name: String,
    trait_generics: Vec<GenericParam>,
    
    methods: Vec<MethodTransform>,
    
    impl_header_source: String,
    impl_header_target: String,
    
    pub_type_to_add: String,
}

struct MethodTransform {
    method_name: String,
    old_param_name: String,  // e.g., "slice"
    new_param_name: String,  // always "self"
    self_mutability: Mutability,  // &self or &mut self
}

struct GenericParam {
    name: String,
    bounds: Vec<String>,
}
```

### Key Challenges

1. **Generic Parameter Renaming**
   - Need to detect conflicts and rename systematically
   - Must update all references throughout the file

2. **Type Extraction**
   - Extract actual type from `&mut [T]` → `[T]`
   - Preserve lifetimes: `&'a mut [T]` → use in pub type
   - Handle complex types: `&mut Vec<T>`, `&[T]`, etc.

3. **Multiple Methods**
   - May have multiple methods with unused self
   - Each may have different data parameters
   - Need to handle all call sites for all methods

4. **Call Site Detection**
   - Must use AST, not string matching
   - Need to distinguish method calls from function calls
   - Extract the actual data argument from each call
   - Handle nested calls correctly

### Testing Strategy

1. Start with simple cases (single method, simple type)
2. Add complexity gradually:
   - Multiple methods
   - Complex generic bounds
   - Lifetimes in types
   - Nested generic types
3. Test on multiple APAS modules beyond InsertionSortSt

### Migration Path

1. Keep InsertionSortSt prototype as `rusticate-fix-insertion-sort-st` (specific tool)
2. Build generic version as separate tool initially
3. Test generic version on InsertionSortSt to verify equivalence
4. Apply to other modules (MergeSortSt, FibonacciMt, etc.)
5. Once proven, deprecate specific prototype

## Next Steps

1. Extend `review-typeclasses` to output structured analysis data
2. Create `TransformationPlan` builder from analysis
3. Implement generic transformations using the plan
4. Test on progressively complex cases

