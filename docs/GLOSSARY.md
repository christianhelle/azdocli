# Glossary

Domain terms used in azdocli's code and design discussions.

## Connection

The resolved organization, credentials and API clients for one azdocli invocation. `main` loads it once (`Connection::load()`) for command families that need a login, and passes `&Connection` down to handlers. It also resolves the project: an explicit `--project` argument wins, otherwise the configured default project. Tests build one with `Connection::new(creds, default_project)`, without touching disk.

_Avoid_: context, session, client factory.
