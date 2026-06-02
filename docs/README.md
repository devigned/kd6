# KD6 Documentation

KD6 is a reference implementation of the Open Memory Service (OMS)
specification, providing structured, searchable, multi-tenant memory for
agentic AI workloads. This directory contains detailed documentation for
developers, operators, and contributors.

## Contents

### [Architecture](architecture.md)

How the system is built. Covers the four-crate workspace structure, the Service
Provider Interface (SPI), data flow between layers, multi-tenancy model,
concurrency strategy, and the reasoning behind key design decisions.

Start here if you want to understand how the pieces fit together or if you are
planning to contribute a new backend.

### [Usage Guide](usage.md)

Getting started with KD6. Covers building from source, running the HTTP server
and MCP server, environment variables, and a walkthrough of the API with curl
examples for every major operation.

Start here if you want to run KD6 and interact with it.

### [Features](features.md)

Complete reference for every feature across all three OMS conformance levels.
Each feature includes a description, the API routes that expose it, and notes on
implementation behavior.

Start here if you want to know what KD6 can do and how each capability works.

### [OMS Specification](specification.md)

Overview of the Open Memory Service specification that KD6 implements.
Explains the memory model, the five layers, the scope hierarchy, conformance
levels, the SPI, and KD6's full conformance matrix.

Start here if you want to understand the specification itself and how KD6
relates to it.

## Additional Resources

- [OMS Specification (full text)](../spec/oms-spec.md) -- the raw specification
  document
- [Copilot Instructions](../.github/copilot-instructions.md) -- project
  conventions and build commands for AI-assisted development
