# Org-mode Timestamp Tests

### TODO [#A] Deadline with warning
`DEADLINE: <2025-12-10 Wed -3d>`

Task that should appear 3 days before deadline.

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
`CLOSED: <2025-12-01 Mon>`

Task that was completed.

### TODO Deadline without warning
`DEADLINE: <2025-12-15 Mon>`

Should use default 14-day warning period.

### TODO Scheduled task
`SCHEDULED: <2025-12-03 Wed>`

Simple scheduled task without repeater.
