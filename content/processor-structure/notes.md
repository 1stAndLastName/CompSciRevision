---
title: Structure and function of the processor
spec: "1.1.1"
sample: true
---

## The main components

The processor (CPU) fetches instructions from main memory, decodes them and carries them out. It has three main parts.

- **Arithmetic and Logic Unit (ALU):** carries out arithmetic such as addition and subtraction, logical operations such as AND and OR, and comparisons. Results normally go into the accumulator.
- **Control Unit (CU):** decodes each instruction and sends out control signals that coordinate the rest of the processor, memory and I/O. It manages the fetch-decode-execute cycle.
- **Registers:** very small, very fast storage locations inside the processor. Each has a specific job.

## Registers

| Register | What it holds |
| --- | --- |
| **Program Counter (PC)** | The address of the *next* instruction to be fetched |
| **Memory Address Register (MAR)** | The address of the memory location about to be read from or written to |
| **Memory Data Register (MDR)** | The data or instruction just read from memory, or waiting to be written to memory |
| **Current Instruction Register (CIR)** | The instruction currently being decoded and executed, split into opcode and operand |
| **Accumulator (ACC)** | The result of calculations done by the ALU |

A common mistake is to say the MAR holds "the next instruction". The MAR holds an **address**, and it can be the address of data as well as of an instruction.

## Buses

A bus is a set of parallel wires that connects the processor to memory and I/O controllers.

- **Address bus:** carries memory addresses from the processor to memory. It is **one-way**. Its width sets how many locations can be addressed: with *n* lines there are 2<sup>n</sup> addresses, so adding one line doubles the addressable memory.
- **Data bus:** carries data and instructions between the processor and memory. It is **two-way**. A wider data bus moves more bits in each transfer.
- **Control bus:** carries control signals such as memory read, memory write, the clock and interrupt requests. It is **two-way**.

**How this relates to assembly language programs.** Every assembly instruction becomes a machine code word made of an **opcode** and an **operand**, and each bus limits what that word can do. When an instruction such as `LDA 7` runs, its operand travels from the CIR to the MAR and out along the **address bus**, so the width of the address bus decides the highest memory address a program can use. The **data bus** carries the instruction itself and any value being loaded or stored, so its width limits how much can move in one transfer. The number of opcode bits limits how many different instructions the processor can have: 5 bits give 2<sup>5</sup> = 32 opcodes.

## The fetch-decode-execute cycle

**Fetch**

1. The address in the PC is copied into the MAR.
2. The PC is incremented so it points to the next instruction.
3. The address is sent along the address bus and a read signal is sent on the control bus.
4. The instruction at that address travels along the data bus into the MDR.
5. The instruction is copied from the MDR into the CIR.

**Decode**

The CU decodes the instruction in the CIR, splitting it into its **opcode** (what to do) and **operand** (what to do it to, often an address).

**Execute**

The instruction is carried out. For example, the assembly instruction `LDA 25` copies the address 25 into the MAR, fetches the value stored there into the MDR, and then copies it into the ACC. `ADD 26` fetches the value at address 26 and the ALU adds it to the ACC. A branch such as `BRA 10` puts a new address into the PC, so the next fetch comes from somewhere else.

## Factors affecting performance

- **Clock speed:** the number of clock cycles per second (measured in hertz). A higher clock speed means more instructions can be executed each second, but it also produces more heat.
- **Number of cores:** each core is a complete processing unit that can run instructions independently. More cores help only when the work can be split into parts that run in parallel, and coordinating the cores adds some overhead. Two cores rarely make a program twice as fast.
- **Cache:** a small amount of very fast memory on or near the processor that stores frequently used instructions and data. Reading from cache is much quicker than reading from main memory. A larger cache means more requests are found in the cache, but cache is expensive and a larger cache is slightly slower to search. Cache is arranged in levels (L1, L2, L3), where L1 is the smallest and fastest.

## Pipelining

In a **pipeline**, the processor works on different stages of several instructions at the same time. While one instruction is being executed, the next is being decoded and the one after that is being fetched. This keeps every part of the processor busy and increases the number of instructions completed per second.

If the executed instruction is a **branch**, the instructions already fetched and decoded may be the wrong ones. The pipeline must be **flushed** (emptied) and refilled from the new address, which wastes cycles.

## Processor architectures

- **Von Neumann architecture:** one memory holds both instructions and data, connected to the processor by one set of buses. Instructions and data cannot be fetched at the same time, which limits performance (the *Von Neumann bottleneck*). It is simpler and cheaper to build and is the basis of general-purpose computers.
- **Harvard architecture:** instructions and data are held in **separate memories** with **separate buses**, so both can be accessed at the same time. The two memories can have different sizes and properties. It is common in embedded systems and digital signal processors.
- **Contemporary processors** combine the two. Main memory is shared as in Von Neumann, but inside the processor there are separate L1 caches for instructions and data, as in Harvard.
