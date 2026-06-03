# KD6 Documentation

KD6 is a reference implementation of the Open Memory Service (OMS)
specification, providing structured, searchable, multi-tenant memory for
agentic AI workloads. This directory contains detailed documentation for
developers, operators, and contributors.

## Contents

### [Architecture](architecture.md)

How the system is built. Covers the five-crate workspace structure, the Service
Provider Interface (SPI), the provider module split, embedding architecture,
data flow between layers, multi-tenancy model, concurrency strategy, and the
reasoning behind key design decisions.

Start here if you want to understand how the pieces fit together or if you are
planning to contribute a new backend.

### [Usage Guide](usage.md)

Getting started with KD6. Covers building from source, running the HTTP server
and MCP server, environment variables (including embedding configuration), and a
walkthrough of the API with curl examples for every major operation.

Start here if you want to run KD6 and interact with it.

### [Features](features.md)

Complete reference for every feature across all three OMS conformance levels.
Each feature includes a description, the API routes that expose it, and notes on
implementation behavior. Covers store name routing, upsert support, server-side
embedding, auto-provisioning, and all Level 1–3 capabilities.

Start here if you want to know what KD6 can do and how each capability works.

### [OMS Specification](specification.md)

Overview of the Open Memory Service specification that KD6 implements.
Explains the memory model, the five layers, the scope hierarchy, conformance
levels, the SPI, and KD6's full conformance matrix.

Start here if you want to understand the specification itself and how KD6
relates to it.

### [Contributing](contributing.md)

Development workflow, coding conventions, testing practices, and commit
guidelines. Covers the Makefile CI pipeline, zero-warnings policy, transaction
discipline, and how to add new backends.

Start here if you want to contribute to KD6.

## Additional Resources

- [OMS Specification (full text)](../spec/oms-spec.md) — the raw specification
  document
- [Squad Memory Example](../examples/squad-memory/) — multi-agent example using
  KD6 as a memory provider
- [Copilot Instructions](../.github/copilot-instructions.md) — project
  conventions and build commands for AI-assisted development
