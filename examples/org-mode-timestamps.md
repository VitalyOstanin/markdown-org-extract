# Org-mode Timestamp Tests

### TODO [#A] Daily standup
`SCHEDULED: <2024-12-01 Sun 10:30-11:00 +1d>`

Repeats every day for testing.

### TODO [#A] Deadline with warning
`DEADLINE: <2025-12-10 Wed -3d>`

The `-3d` cookie overrides the global 14-day warning window: this
task appears in the upcoming bucket only from three days before the
deadline. Units `h/d/w/m/y` are accepted; see `parse_org_timestamp`
and ADR-0002 for the full list.

### TODO [#B] Weekly repeating task
`SCHEDULED: <2025-12-01 Mon +1w>`

Repeats every Monday.

### TODO [#C] Daily repeating task
`SCHEDULED: <2025-12-01 Mon +1d>`

Repeats every day.

### TODO Conference event
`<2025-12-20 Sat>--<2025-12-22 Mon>`

Multi-day event spanning weekend.

### TODO Meeting with time
`<2025-12-05 Fri 14:00-15:30>`

Meeting with specific time slot.

### DONE Completed task
`CLOSED: [2025-12-01 Mon]`

Task that was completed.

### TODO Deadline without warning
`DEADLINE: <2025-12-15 Mon>`

Should use default 14-day warning period.

### TODO Scheduled task
`SCHEDULED: <2025-12-03 Wed>`

Simple scheduled task without repeater.
