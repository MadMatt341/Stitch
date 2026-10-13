# Documentation rules

Policy version: `DocumentationRules` 0.1.1.
These rules include reading and writing documentation for Stitch.
The [contributor task map](../AGENTS.md#read-for-the-task) gives the local reference owners and check entry points.

## Read for the task

Start with the contributor task map.
Search the owning reference for the applicable subject or heading.
Read the full applicable section and its conditions.
A search result gives a location.
It does not give the full contract.

Before an implementation change, find this information:

- The component with state ownership
- The necessary behavior
- The interfaces and dependencies that the change can change
- The applicable failure behavior and compatibility conditions
- The applicable checks and their environment limits.

Read the applicable source and tests to examine this information.
If a different owner has necessary information, read that owning reference.
If evidence and implementation guidance are different, record the difference.
Use dated evidence only for its specified build and environment.
When you have the necessary implementation and check information, stop reading more references.
For a small text correction, read only its related context and writing rules.

## Write at the owner

Keep one owning reference for each contract.
Use links to it from related documents.
Put implemented behavior, conditions, and checks before dated history.
Keep failures, prerequisites, and recovery with the procedure that uses them.
Give the cause of a design selection when it can prevent an incorrect implementation.
Give proposals and open issues clear labels.

Change the owning reference with a change to its contract.
Correct related overview and status text in the same change.
A new history entry cannot correct guidance that gives different contract information.

Give procedures their working directory and literal commands.
Give their expected result and applicable recovery steps.
Give the effects of commands that change an environment or installation.
Keep different records for implemented behavior, automated checks, staged files, running builds, and observed acceptance.

## Keep more information easy to find

Use these prose word-count estimates as initial word counts for a check:

| Part | Word count for a check |
| --- | --- |
| Contributor guide | Use 500 to 800 words as an initial target. Examine it when it has more than 1,000 words. |
| Routing summary | Use 100 to 200 words as an initial target. |
| Topic section | Use 300 to 800 words as an initial target. Examine a longer section for different reader tasks. |

Do not count fenced code or link destinations in these estimates.
These counts are not maximum word counts, token budgets, or ASD-STE100 sentence counts.
Do not add words only to increase a word count.

If division removes a necessary condition from the reading route, keep the full contract together.
Do not remove necessary information to obey a word-count target.
For a different reader task, move information to that task's owning reference.
Give the parent a scope description, a task condition, and a link.
Keep source and test entry points at the owner.
When you move text, keep previous routes or correct their callers.

Keep long dated results in linked evidence references.
Give each result its date, build, environment, and remaining checks.
Keep raw logs and generated reports in ignored output directories.

## Obey ASD-STE100

Write all new and changed documentation in ASD-STE100 Simplified Technical English, Issue 9.
Obey the full writing rules and dictionary in the [official standard](https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf).
If the user supplies the official local PDF, use it.
Do not copy the standard into this project.

Use approved words with their approved meanings, parts of speech, and forms.
Use technical nouns and verbs only in the categories given in rules 1.5 and 1.12.
If an approved verb gives the same meaning, use that verb.
If a technical noun is not clear, give its meaning.
Use the same technical noun for the same item.

Use American English spelling.
Use active voice.
You can use passive voice in descriptive text only when the agent is unknown.

Do not use more than 20 words in a procedural sentence.
Do not use more than 25 words in a descriptive sentence.
Obey the word-count rules in section 8.
Give one instruction in each sentence.
Put its necessary condition first.

Keep each paragraph about one subject.
Obey the six-sentence limit for descriptive paragraphs in rule 6.6.
Do not change literal commands, paths, identifiers, API names, UI labels, or quotations to obey STE.
Examine new and changed prose against the full standard before you complete the change.

A word checker or sentence counter cannot show full compliance.
If you cannot complete a necessary check, record its cause and remaining scope.

## Complete and change rules

Examine related links, anchors, source and test paths, and Git whitespace.
Run `git diff --check` for a documentation-only change.
When partial staging changes the content, examine the proposed commit independently.
For implementation changes, use the owner's necessary checks.
Do not change work that is not in the task scope.
Do not change the evidence.

Use documentation failures from tasks to change these rules.
Record the problem and evidence at its owner.
Change only the applicable rule.
When a rule replaces a previous rule, remove the previous rule.

Record the cause and policy version in the shared version history.
Before this project uses a new version, examine related routes.
Record that version in this guide.
Keep the user's selections and approval scope.
