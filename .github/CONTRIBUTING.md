# Contributing

You can contribute to this repository in several ways.

## Contributions

### Improve Documentation

Since this is a very early version of the repository, any additions to existing documentation are GREATLY appreciated.

### Give Feedback

If you feel strongly about a proposed design or feature, or are interested in influencing the roadmap of features, let us know.

### Write Code

Adding tests, eliminating defects, implementing features are all fantastic ways to contribute. Of course, with prior discussion and approval.

## Doc Comment Style

- Use American English spelling throughout (e.g., "color", "honor", "analyze").
- Limit em-dashes (—); prefer commas or parentheses.
- Keep doc comments concise: describe **what** an item does, not **why** or **how**.
- Do not include implementation details, examples, file paths, or algorithm internals in doc comments; move those to `CLAUDE.md`, inline comments, or test names instead.
- Avoid "Test T0XX:" or "Regression test for issue #XXX:" prefixes; the test name should be self-explanatory.
- Doc comments document the public API. Private helpers and tests can use brief inline comments instead.

## Terminology/Tips

We have very specific terminology when dealing with contributions, specifically bugs. This is to minimize confusion between contributors when discussing code.

- Defect: incorrect program *code*
- Infection: incorrect program *state*

The infection propagates, causing a failure.

- Failure: an observable incorrect program behavior or program execution

Failures are also called "issues" or "problems".

When bug-hunting, remember the TRAFFIC principle:

- **T**rack the problem
- **R**eproduce
- **A**utomate
- **F**ind origins
- **F**ocus
- **I**solate
- **C**orrect

## Resources

- [How to Contribute to Open Source](https://opensource.guide/how-to-contribute/)
- [Making your first contribution](https://medium.com/@vadimdemedes/making-your-first-contribution-de6576ddb190)
