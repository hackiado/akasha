# Disable default file completion at top-level
complete -c ak -f

# Top-level subcommands
complete -c ak -n "__fish_use_subcommand" -a init -d "init data"
complete -c ak -n "__fish_use_subcommand" -a inscribe -d "track data from a path into the current cube"
complete -c ak -n "__fish_use_subcommand" -a seal -d "register a commit into the current cube"
complete -c ak -n "__fish_use_subcommand" -a timeline -d "show event timeline (commits)"
complete -c ak -n "__fish_use_subcommand" -a view -d "show the latest commit"
complete -c ak -n "__fish_use_subcommand" -a diff -d "show changes since the last seal"

# --- inscribe ---
# Positional path (optional) — suggest directories
complete -c ak -n "__fish_seen_subcommand_from inscribe" -a "(__fish_complete_directories)" -d "Path to scan (default: .)"

# --- seal ---
# -t/--type with suggestions
complete -c ak -n "__fish_seen_subcommand_from seal" -s t -l type -r -a "feat fix refactor docs test chore" -d "Commit type"
# -s/--summary requires a value
complete -c ak -n "__fish_seen_subcommand_from seal" -s s -l summary -r -d "Commit summary"
# -b/--body requires a value
complete -c ak -n "__fish_seen_subcommand_from seal" -s b -l body -r -d "Commit body"

# --- timeline ---
complete -c ak -n "__fish_seen_subcommand_from timeline" -l utc -d "Display timestamps in UTC"
complete -c ak -n "__fish_seen_subcommand_from timeline" -l iso -d "Display timestamps in ISO 8601"

# --- view ---
# no flags/args

# --- diff ---
# no flags/args