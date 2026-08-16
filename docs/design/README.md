# Design Specifications

Design docs describe *what* LAR is and *why*. Implementation details live under [../implementation/](../implementation/).

**Status key**

| Marker | Meaning |
|--------|---------|
| Implemented | Shipped; see linked implementation docs |
| Partial | Some pieces shipped; remainder planned |
| Planned | Design intent only; not built yet |
| Design | Foundational intent (always current) |

| Doc | Topic | Status |
|-----|--------|--------|
| [introduction.md](introduction.md) | Purpose and scope | Design |
| [principles.md](principles.md) | Native Linux, immutability, independence | Design |
| [architecture.md](architecture.md) | Package sources, store, resolver, install | Partial |
| [packages.md](packages.md) | Package model, identity, manifest | Implemented |
| [dependency-resolution.md](dependency-resolution.md) | Resolving dependencies | Implemented |
| [runtime.md](runtime.md) | Runtime composition, linking, launch | Implemented |
| [desktop-and-services.md](desktop-and-services.md) | Desktop integration and services | Partial |
| [platform.md](platform.md) | OS boundary, security, updates, rollback | Partial |
| [future-extensions.md](future-extensions.md) | Possible future work | Planned |
| [summary.md](summary.md) | Design summary | Design |
