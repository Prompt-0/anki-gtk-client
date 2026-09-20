project('rookery', 'rust',
  version: '0.1.0',
  meson_version: '>= 0.59.0',
)

cargo = find_program('cargo', required: true)

run_target(
  'build-cargo',
  command: [cargo, 'build', '--release']
)
