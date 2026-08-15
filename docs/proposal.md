# LAR (Linux Application Runtime)

## Project Proposal

This document is the project pitch: problem, goals, and benefits. Detailed design lives under [design/](design/); shipped formats and subsystems under [implementation/](implementation/).

---

## Overview

LAR (Linux Application Runtime) is a Linux application platform that decouples application lifecycle from operating system lifecycle.

It provides Side-by-Side (SxS) package storage, dependency resolution, and per-application runtime environments while keeping native Linux execution (ELF, dynamic linker, standard desktop protocols).

The Linux platform provides the foundation (kernel, hardware, system services, desktop). LAR manages packaging, resolution, and application runtimes. Applications declare their requirements; LAR resolves the environment.

---

## Problem Statement

Traditional Linux distributions manage the OS and application ecosystem together. That leads to:

- Applications tied to distribution-specific libraries
- Conflicting dependency version needs across apps
- Application updates blocked on distribution release cycles
- Large application maintenance burden on distributors
- Weak long-term application compatibility

---

## Goals

- Decouple applications from distribution releases
- Provide reproducible application runtimes
- Support multiple coexisting dependency versions
- Preserve native Linux execution behavior
- Keep standard desktop and service integration
- Reduce distribution application-maintenance burden
- Allow OS and applications to release at different speeds

---

## High-level shape

Three layers:

| Layer | Owns |
|-------|------|
| Linux platform | Kernel, drivers, system services, desktop, hardware |
| LAR | Packages, SxS store, resolution, runtimes |
| Application ecosystem | App repositories, packages, updates |

For component responsibilities (repos, registry, store, resolver), package model, runtime, security, and related topics, see the [design specifications](design/).

---

## Benefits

**Users** — fewer dependency conflicts, safer updates, better compatibility, multiple versions can coexist.

**Developers** — less distro-specific packaging, predictable environments, native Linux compatibility, faster releases.

**Distributions** — less application maintenance, focus on platform stability, easier OS upgrades.

Applications can ship on their own cadence; distributions focus on kernel, hardware, desktop, and system services.

---

## Core principle

Applications should run as native Linux applications while having predictable, reproducible, and independent runtime environments.

---

## Further reading

| Doc | Role |
|-----|------|
| [design/](design/) | Architecture and intent (source of truth for design) |
| [implementation/](implementation/) | Concrete formats and shipped subsystems |
| [design/summary.md](design/summary.md) | Short design summary |
