# Axus Implementation Plans

This document covers in-progress work and its dependencies. The procedures of each work live in the child documents, and the post-completion state in the code and the git history.
Add a row when starting work, advance its state, and delete the row and child document once the completion conditions are met and the plan is folded.

## 1. About this document

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | In-progress work and dependencies |
| [design.md](./design.md#11-division-of-responsibilities-among-documents) | The map of the document family and permanent design decisions |

## 2. List

| Plan | Purpose | State | Prerequisite |
| --- | --- | --- | --- |
| [file-exchanger-block-exchange](./plans/file-exchanger-block-exchange.md) | Two daemons exchange blocks and a mismatched block is never stored | in progress | - |
| [file-exchanger-transfer-efficiency](./plans/file-exchanger-transfer-efficiency.md) | The fetch ignores the peers holding nothing and the slow peers at the tail | in progress | - |
