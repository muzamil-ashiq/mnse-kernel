# K Language Workflow Pipeline Test
recent.commands | where $it.exit_code == 101 | select command, exit_code
