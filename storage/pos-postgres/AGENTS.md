# pos-postgres: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

PostgreSQL implementation for authoritative server-side POS state.

## Boundaries

Keep database-specific SQL here; do not weaken tenant or workspace isolation.
Currently the only persistent POS-owned record is a server-instance marker.
Do not add Task, Project, user, tenant or credential tables until scoped
ownership and the authentication model are explicitly designed.

## Verification

The PostgreSQL connection/migration foundation has unit tests and a CI
integration test on a disposable localhost database. Future data repositories
must test transactions, concurrent writes, permissions and tenant isolation.
The crate is not yet connected to the HTTP server or to business data.
