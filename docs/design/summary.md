# Design Summary

**Status:** Design

LAR introduces a new application management layer for Linux.

The architecture separates:

**Linux Platform**

- Kernel.
- Drivers.
- Desktop.
- System services.

**LAR**

- Package management.
- Runtime resolution.
- Dependency management.

**Applications**

- Independent software lifecycle.
- Own declared requirements.

The core principle:

Applications should run as native Linux applications while having predictable, reproducible, and independent runtime environments.
