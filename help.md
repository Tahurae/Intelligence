# ILang (Ingenious) — Complete Reference Manual

This manual provides a full breakdown of the ILang language, its file structure, core rules, built-in tools, and terminal shortcuts.

---

## 1. Core Philosophy

ILang is designed around a simple idea: **Software is a set of pathways carrying data between processing stations.**

Instead of telling the computer step-by-step how to manipulate memory, you draw a map of how information flows:
* **Pathways** carry specific types of data.
* **Stations** transform data from one pathway into another.
* **Connections** link stations together sequentially or in parallel.

---

## 2. Language Keywords & Syntax

### Pathways (`type`)
A pathway defines a specific kind of data moving through your program.

```ilang
type KeyStream;
type TextBuffer;
type DiskRecord;
```

### Stations (`morphism`)
A station processes incoming data from one pathway and produces data for an outgoing pathway.

```ilang
morphism ProcessKey : KeyStream -> TextBuffer;
morphism SaveToDisk : TextBuffer -> DiskRecord;
```

### Pipelines (`pipeline`)
A pipeline connects stations into a complete program flow.

```ilang
pipeline MainProgram = ProcessKey >> SaveToDisk;
```

---

## 3. The Core Connection Rules

ILang uses four primary operators to control how pathways and stations interact:

| Operator | Name | What It Does |
| :--- | :--- | :--- |
| **`>>`** | Sequential Flow | Connects stations end-to-end. Output from the left station enters the right station. |
| **`&#124;&#124;`** | Parallel Flow | Runs two pathways or stations side-by-side simultaneously without interference. |
| **`*`** | Matrix Binding | Merges two pathways into a combined state grid for advanced data storage. |
| **`~`** | Reset / Unbind | Cleans up linked states and frees system resources when work is complete. |

---

## 4. Project File Structure

The repository is organized into four clean directories:

* **`src/` (The Compiler):** The immutable core engine written in Rust. It checks your pathways for errors and converts your `.i` blueprints into fast C code.
* **`std/` (Standard Library):** Reusable ILang files containing basic building blocks and mathematical transformations.
* **`extensions/` (System Driver Substrates):** Low-level C files that handle hardware operations, terminal keypresses, and file writing.
* **`examples/` (Sample Programs):** Ready-to-compile `.i` blueprints showing how to build real applications.

---

## 5. Built-in Tool: The `note` Editor & Shortcuts

ILang includes a globally available terminal note-taking editor (`note`) built entirely using these concepts.

* **Binary Location:** `$PREFIX/bin/note`
* **Storage File:** `~/.ilang_repository.txt`

### Termux Commands
* **`note`**: Launches the global ILang text/note editor from anywhere in the terminal.
* **`notes`**: Directly views and prints all saved note logs from your repository file.

### In-Editor Keyboard Shortcuts
* **`Ctrl + S`**: Save current note with an automatic timestamp and active tag.
* **`Ctrl + R`**: Open and read previously saved notes inside the editor.
* **`Ctrl + T`**: Switch or create a new organizational tag/category.
* **`Ctrl + X`**: Exit the editor and return to terminal prompt.

---

## 6. How ILang Executes Code

1. You write a program blueprint in an **`.i`** file.
2. The ILang compiler reads the file and validates every connection.
3. ILang generates clean C code (**`payload.c`**).
4. Your system compiler (`gcc` or `clang`) turns the C code into a tiny, standalone program that runs anywhere without extra requirements.
