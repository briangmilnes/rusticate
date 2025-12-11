# Native Parsers for Proof Systems

## Overview

| System | Language | Parser Tech | Repository |
|--------|----------|-------------|------------|
| Coq | OCaml | Menhir (LR) | github.com/coq/coq |
| Dafny | C# | ANTLR 4 | github.com/dafny-lang/dafny |
| F* | F# + OCaml | FsLexYacc | github.com/FStarLang/FStar |
| Isabelle | Standard ML | Custom combinators | github.com/isabelle-prover/isabelle |
| Lean 4 | Lean (self-hosted) | Pratt + macros | github.com/leanprover/lean4 |

## Coq
- **Language**: OCaml
- **Parser**: Menhir LR parser generator
- **Location**: `parsing/` directory (g_vernac.mlg, g_constr.mlg)
- **Features**: Extensible grammar, notation system, dependent types

## Dafny
- **Language**: C#
- **Parser**: ANTLR 4 (migrating from Coco/R)
- **Location**: `Source/DafnyCore/Dafny.atg` and `AST/`
- **Features**: Verification-aware syntax (requires, ensures, invariant)

## F*
- **Language**: F# + OCaml
- **Parser**: FsLexYacc (F# port of OCamllex/Ocamlyacc)
- **Location**: `src/parser/` (lex.fsl, parse.fsy)
- **Features**: Dependent types, refinements, effect system

## Isabelle
- **Language**: Standard ML (Poly/ML)
- **Parser**: Custom combinator-based
- **Location**: `src/Pure/Isar/` and `src/Pure/Syntax/`
- **Features**: Generic (multi-logic), structured proofs (Isar)

## Lean 4
- **Language**: Lean (self-hosted) + C++ kernel
- **Parser**: Pratt parser + hygienic macro system
- **Location**: `src/Init/Prelude.lean`, `src/Lean/Parser/`
- **Features**: User-extensible syntax, metaprogramming, elaborator

## For Verus: What to Borrow

1. **Dafny**: Inline verification annotations (`requires`, `ensures`)
2. **F***: Explicit effect tracking, refinement types
3. **Lean 4**: User-extensible syntax, powerful elaboration
4. **Isabelle**: Structured proof language (Isar-like blocks)
5. **Coq**: Tactic language, proof automation

## Building Rust Parsers

**Feasibility**:
- Dafny: Medium (ANTLR grammar exists) - 2-3 months
- F*: High (grammar scattered) - 4-6 months  
- Coq: Very High (complex, extensible) - 6+ months
- Isabelle: Very High (multi-language) - 6+ months
- Lean 4: Extreme (self-hosted, extensible) - Very difficult

**Better Approach**: Use native tooling via FFI, LSP, or AST export formats.

## References
- Aeneas (Rust→Proof): github.com/AeneasVerif/aeneas
- Hax (Rust→Proof): github.com/cryspen/hax
