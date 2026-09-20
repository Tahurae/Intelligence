# Intelligence Programming Language

> **A Monoidal Category Language (`.i`) lowering string diagrams to C99 for zero-dependency binary execution.**

---

## 1. How Any Software Works

Every computer program does three fundamental things:
1. **Takes in information** (Input)
2. **Transforms that information** (Processing)
3. **Delivers the result** (Output)

Instead of writing long, complex lists of step-by-step commands, **Intelligence** lets you design software by defining how data moves through connected pathways. Information enters a pathway, gets transformed at a specific station, and moves forward to the next step.

---

## 2. The Fundamental Building Blocks

To build any software system in Intelligence, you only need to understand three core ideas:

### Data Pathways
A pathway carries a specific type of information through your system. It ensures that the right kind of data reaches the right destination safely.

### Transformation Stations
A station takes data from an incoming pathway, changes or processes it, and releases it onto an outgoing pathway.

### The Three Connection Rules

* **Sequential Flow (`>>`):** Connects stations in a row. The result of the first station feeds directly into the input of the next station.
* **Parallel Flow (`||`):** Runs two stations side-by-side at the same time. Data travels through both independent pathways simultaneously without interference.
* **Reset and Release (`~`):** Clears used memory or resets state after a process finishes, keeping your system running fast and clutter-free.

---

## 3. How Intelligence Runs Your Code

Intelligence builds software using a simple **three-step process**:

1. **Write the Blueprint (`.i` file):** Describe your pathways, stations, and connection rules in plain Intelligence text.
2. **Verify the Connections:** The Intelligence core engine reads your blueprint and checks that every input and output matches up properly without errors.
3. **Generate a Standalone Program:** Intelligence converts your blueprint into standard C code and creates a tiny, fast program file. This final program runs directly on your machine without requiring extra software or runtime dependencies.

---

## 4. Installation & Setup

### Quick Install
Run this command in your terminal to download and install the software:

```bash
curl -sSL https://raw.githubusercontent.com/Tahurae/Intelligence/main/install.sh | sh
```

### Build from Source
If you have Rust installed on your system:

```bash
git clone https://github.com/Tahurae/Intelligence.git
cd Intelligence
cargo build --release
```
