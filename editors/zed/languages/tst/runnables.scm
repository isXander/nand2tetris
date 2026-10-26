; Run the script from the gutter next to its `load` command.
(
  (command
    body: (load_command
      "load" @run)) @_load
  (#set! tag n2t-script)
)
