# Type History agent skills

The catalog source is in [catalog](catalog/). Catalog `type-history`, stack
`type-history`, and bundle `type-history-skills` group the decision skills below.
Each skill stands alone.

| Skill | Use it to decide |
| --- | --- |
| [Evolution decisions](catalog/skills/type-history-evolution-decisions/SKILL.md) | Whether a change belongs in a history and what older values can truthfully become |
| [Compatibility evidence](catalog/skills/type-history-compatibility-evidence/SKILL.md) | What proves that retained payloads still decode and convert correctly |
| [Release boundaries](catalog/skills/type-history-release-boundaries/SKILL.md) | When to freeze a schema and when readers and writers can switch versions |

The [README](../README.md), [book](../book/README.md), and
[Rustdoc](https://docs.rs/type-history/) own APIs, attributes, generated names,
setup, commands, supported types, and worked examples. Skills guide judgment.
General application-code review remains outside this catalog's scope.

Skill links use absolute repository URLs so they remain usable after delivery
outside this checkout. They point to `main`. Consult documentation at the
consumer's locked Type History revision when it differs.

## Validation

From the repository root, set `TMPDIR` to an existing writable directory for
disposable validation files, then run:

```sh
bash .github/scripts/check-skillcatalog.sh
python3 .github/scripts/check-skill-links.py
```

The catalog check downloads a checksum-pinned SkillCatalog validator and uses
disposable configuration. It does not change installed profiles. The link check
resolves skill links and heading anchors against this checkout. Both run in CI.
These checks validate catalog source, not client installation or agent judgment.

When guidance changes, verify its claims against current documentation, code,
and tests. Exercise a realistic case that tempts the wrong decision and a valid
case that should pass. Keep the outcomes with the change's review evidence.
