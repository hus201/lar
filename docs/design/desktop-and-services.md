# Desktop and Services

**Status:** Partial — `.desktop` publish, PATH exports (native trampoline), and `lar launch` are implemented; MIME, portals, D-Bus activation, and service metadata remain planned — [desktop.md](../implementation/desktop.md)

## Desktop Integration

The Desktop Environment remains part of the operating system.

LAR applications integrate through standard Linux interfaces:

- Wayland.
- D-Bus.
- Desktop Entry specification (v1: `.desktop` publish + PATH exports; `lar launch` for admin/debug).
- MIME handling.
- Notifications.
- Portals.

Applications should not depend on desktop implementation internals.

## Service Application Support

LAR supports non-GUI applications.

Application categories include:

- Desktop Applications.
- CLI Applications.
- Background Applications.
- Service Applications.

Examples:

- PostgreSQL.
- Redis.
- Nginx.
- RabbitMQ.

Service applications use:

- Application packages.
- Runtime resolution.
- Service metadata.

The operating system service manager manages lifecycle.

LAR provides the runtime environment.
