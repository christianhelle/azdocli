# Migrate
The `migrate` command clones one or more Azure DevOps team projects from a source organization to a target organization. It supports a single-project run or a batch run driven by a JSON manifest, and it records migration artifacts such as state, ID maps, exports, and reports under the selected output directory.

## Prerequisites

Create two named credential profiles, one for each side of the migration. You can use either cloud organizations or enterprise/on-premises Azure DevOps base URLs when logging in. For enterprise servers, provide the base URL without the organization or collection segment (for example, `https://devops.mycompany.com` or `https://tfs.mycompany.com/tfs`), or paste the full URL including the collection (for example, `https://tfs.mycompany.com/tfs/DefaultCollection`) so the last path segment is used as the organization/collection:

```sh
azdocli login --profile source
azdocli login --profile target
```

Use PATs with the scopes needed for the assets you migrate: **Code** read & write, **Build** read & execute, **Work Items** read & write, and **Project and Team** read. The target account also needs permission to create target projects when using `--create-target`; work item migration requires the Azure DevOps **Bypass rules on work item updates** permission.

## Single-project usage

```sh
azdocli migrate project --source-profile source --target-profile target --source SourceProject
```

```sh
azdocli migrate project \
  --source-profile source \
  --target-profile target \
  --source SourceProject \
  --target TargetProject \
  --create-target \
  --dry-run \
  --resume
```

Useful flags include `--phases <PHASES>`, `--skip-phases <SKIP_PHASES>`, `--fail-fast`, `--state-file <STATE_FILE>`, `--output-dir <OUTPUT_DIR>`, `--concurrency <CONCURRENCY>`, and `--yes`.

## Batch usage

```json
{
  "source_profile": "source",
  "target_profile": "target",
  "output_dir": "./migrations/",
  "default_options": {
    "create_target": true,
    "concurrency": 4,
    "fail_fast": false,
    "skip_phases": ["dashboards"]
  },
  "projects": [
    { "source": "ProjA", "target": "ProjA" },
    {
      "source": "ProjB",
      "target": "ProjB-Migrated",
      "options": { "skip_phases": ["test_plans"] }
    }
  ]
}
```

```sh
azdocli migrate batch --config manifest.json --resume --yes
```

Batch runs also support `--dry-run`, `--fail-fast`, `--resume`, and `--yes`.

## Fidelity contract

| Asset | Fidelity | Notes |
|---|---|---|
| Project | Full | Creates the target project when `--create-target` is set; copies name, visibility, and description. Target tenant must already have a compatible process. |
| Process template | Export-only | Currently writes a placeholder `process-export.json`; process clone/import is not automated. |
| Area paths | Full | Recreates the area path tree and records path mappings. |
| Iteration paths | Full | Recreates the iteration path tree with attributes, including dates when returned by Azure DevOps. |
| Teams | Partial | Creates teams and maps IDs; members are not migrated because identities do not map cross-tenant. |
| Team board config | Partial | Copies team field values, team iterations, team settings, and board columns/rows to the migrated teams. Iterations that could not be mapped to a target iteration id are skipped with a warning. |
| Repos (git) | Full | Uses `git clone --mirror` and `git push --mirror`; target repositories must be empty and Git LFS is not handled. |
| Wiki | Full | Mirrors the project wiki backing repository; target wiki backing repo must be empty. |
| Work items | Partial | Recreates the latest revision of each work item (standard and `Custom.*`/`MyCustom.*` fields) with area and iteration paths remapped, identities as plain text, and bypass-rules writes. Revision history is not replayed; a summary of the last 50 updates is appended to the description. |
| Work item links | Partial | Recreates links between migrated work items; cross-project and external links are skipped with a warning. |
| Work item attachments | Full | Downloads each attachment from the source and re-uploads it to the target work item, rewriting inline attachment references. |
| Work item comments | Partial | Re-posts non-deleted comments in order, annotated with the original author and date. |
| Pull requests (active) | Partial | Recreates active PRs with the original title, draft state and a migrated-from description. The source and target branches must already exist on the target. Reviewers, comments, labels, linked work items and completion options are not carried over. |
| Pull requests (closed/abandoned/completed) | Export-only | Archived to `prs-closed-archive.json` (including commit lists); not recreated on the target. |
| Variable groups (non-secret) | Partial | Exports each group to JSON and recreates variable groups; secret values are blanked with warnings. |
| Service connections | Export-only | Exports service connection JSON only; manual reconfiguration is required on the target. |
| YAML pipelines | Partial | Recreates YAML pipelines (non-YAML pipelines are skipped) against the migrated repositories and restores variable group references; supports `--dry-run`. Requires the pipeline's repository to be migrated first, and service connection references are not remapped (a warning is logged). |
| Classic pipelines | Partial | Exports classic build and release definitions to JSON and recreates them on the target. Agent pools/queues, service connections and task/extension references are not remapped; warnings are logged so they can be verified on the target. |
| Test plans | Export-only | Exports test plans to JSON under the output directory; manual import is required. |
| Dashboards | Export-only | Exports team dashboards to JSON under the output directory; manual import is required. |

## Resumability

Each run writes a `state.json` file in the migration output directory, or to `--state-file` when specified. Re-run with `--resume` to skip phases already marked done and continue from the saved state.

## Out of scope

The migration does not migrate permissions/security groups, repo permissions, branch policies, approvals/checks/environments, service hooks, artifacts/feeds, shared queries, agent pools/queues, identity or group membership mapping, secrets, or Git LFS objects.

## Known limitations

- `git push --mirror` is destructive, so the implementation refuses to push when the target repository or wiki backing repository is not empty.
- Work item migration requires the Azure DevOps **Bypass rules on work item updates** permission.
- Secrets are not migrated. Variable group secrets are blanked, and service connections are exported for documentation/manual recreation only.

