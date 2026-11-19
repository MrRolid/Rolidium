# Security policy

## Supported versions

Only the **latest release** of Rolidium receives fixes. Rolidium is alpha software; please check that a problem still exists in the
latest release before reporting it.

## Reporting a vulnerability

Please report security problems **confidentially**, not in a public issue:

* through GitHub's private vulnerability reporting: open the [Security tab](https://github.com/MrRolid/Rolidium/security) of the
  repository and choose **Report a vulnerability**, or
* by e-mail to **connect@rolid.com**.

Examples of what counts: a crafted `.rolidium`, STEP or script file that makes Rolidium run code, write files outside the place you
chose, or read files it should not; a way to make `rolidium-mcp` reach files outside its `--root` directory, run commands or use the
network; problems in the release packages themselves.

## What to include

* the Rolidium version (status bar or `rolidium --version`) and the operating system,
* a description of the problem and its impact,
* steps to reproduce it, ideally with a small example file or script,
* whether the problem is already known to others or published anywhere,
* how you would like to be credited (or that you prefer not to be named).

## What to expect

* We will acknowledge your report and keep you informed about what we find and what we plan to do.
* Rolidium is developed by a small team, so we cannot promise fixed response or fix times.
* Please give us a reasonable time to release a fix before you publish details. We will credit you in the release notes if you wish.
