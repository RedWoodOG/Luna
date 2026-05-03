# Temporal Benchmark Rationale

This file records the authorial timestamp decisions for PR 0.1b.

These rationales are based on the human stories provided on 2026-05-03. In those stories, "recent" meant a live emotional context being reactivated within days or weeks, "today/now" meant same-day pressure, and older similar events served as background rather than the active answer.

The purpose of these decisions is not to improve TCF scores. The purpose is to give the temporal benchmark cases explicit event-time meaning before they become proof-eligible.

## recent_stress_001

case_id: recent_stress_001

temporal meaning being tested:
The case tests whether Luna treats a job-stress disclosure from the previous week as the active recent stressor when the later probe asks what was stressing the user recently.

chosen disclosure timestamp:
2026-04-27T09:00:00Z

chosen probe timestamp:
2026-05-03T10:00:00Z

gap duration:
6 days and 1 hour

why that gap matches the prose:
The benchmark says "Last week I was struggling with my job" and later asks "Recently, what was stressing me?" A gap of just under a week matches the ordinary human meaning of recent job stress. This is calibrated by the user's 2026-05-03 work reflection, where thinking about work tomorrow reactivated job stress from the preceding months and recent work events.

why this gap was chosen before seeing recall scores:
The gap was chosen from the wording "last week" and "recently," plus the user's own human example of work stress being reactivated on 2026-05-03. It was not chosen from engine output.

## recent_stress_002

case_id: recent_stress_002

temporal meaning being tested:
The case tests whether Luna prefers a same-day work pressure over an older unrelated anxiety when asked about the recent pressure.

chosen disclosure timestamp:
2026-05-03T08:00:00Z for the same-day client deadline disclosure. The older moving-apartments disclosure should remain months earlier in the case timeline.

chosen probe timestamp:
2026-05-03T14:00:00Z

gap duration:
6 hours between the active pressure disclosure and the probe.

why that gap matches the prose:
The benchmark says "This morning the client deadline had me tense" and later asks "What was the recent pressure?" Same-day pressure should dominate over the older "months ago" anxiety. This follows the user's Chris/cofounder story, where same-day pressure on 2026-05-03 was the active emotional context even though it echoed older AI-project experiences.

why this gap was chosen before seeing recall scores:
The gap was chosen from the phrase "this morning" and the later same-day probe. It was not selected by looking at TCF, keyword, or embedding behavior.

## recent_stress_003

case_id: recent_stress_003

temporal meaning being tested:
The case tests whether Luna prefers a yesterday disclosure over an earlier-year worry when asked what bothered the user lately.

chosen disclosure timestamp:
2026-05-02T16:00:00Z for the manager-feedback disclosure. The earlier tuition worry should remain earlier in the year.

chosen probe timestamp:
2026-05-03T10:00:00Z

gap duration:
18 hours between the active disclosure and the probe.

why that gap matches the prose:
The benchmark says "Yesterday my manager's feedback was what bothered me" and later asks "What bothered me lately?" A next-day probe makes yesterday's feedback the active recent answer, while the tuition worry remains older background. This is consistent with the user's work stories, where feedback from authority figures carried emotional weight and could remain active into the next day.

why this gap was chosen before seeing recall scores:
The gap was chosen from the explicit word "Yesterday" and the ordinary meaning of "lately." It was not chosen from benchmark results.

## recent_stress_004

case_id: recent_stress_004

temporal meaning being tested:
The case tests whether Luna treats a same-day current stressor as more relevant than an older draining situation when the probe asks what is wearing the user down now.

chosen disclosure timestamp:
2026-05-03T09:00:00Z for the budget-review disclosure. The commute disclosure should remain a while back in the case timeline.

chosen probe timestamp:
2026-05-03T14:00:00Z

gap duration:
5 hours between the active disclosure and the probe.

why that gap matches the prose:
The benchmark says "Today the budget review is what's wearing me down" and later asks "What is wearing me down now?" Same-day timing makes the budget review the current answer, not the older commute. This mirrors the user's 2026-05-03 state: the active pressure was what was being thought about now, while older events formed background.

why this gap was chosen before seeing recall scores:
The gap was chosen from "Today" and "now." It was not based on recall output or any engine comparison.

## recent_stress_005

case_id: recent_stress_005

temporal meaning being tested:
The case tests whether Luna prefers a this-week stressor over a last-month worry when asked for the current stressful thing.

chosen disclosure timestamp:
2026-04-30T12:00:00Z for the product-launch disclosure. The travel anxiety should remain one month earlier in the case timeline.

chosen probe timestamp:
2026-05-03T10:00:00Z

gap duration:
2 days and 22 hours between the active disclosure and the probe.

why that gap matches the prose:
The benchmark says "This week the product launch is the stressful thing" and later asks "What's the current stressful thing?" A three-day gap keeps the product launch inside the current week while separating it from last month's travel anxiety. This is calibrated by the user's Chris/cofounder story, where product/work pressure from 2026-04-30 was still active on 2026-05-03.

why this gap was chosen before seeing recall scores:
The gap was chosen from "This week" and "current," plus the user's human example of a 2026-04-30 collaboration still being emotionally active on 2026-05-03. It was not chosen from engine output.
