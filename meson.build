project('pkgbuild_manager', 'rust', 
          version: '2.2.0',
    meson_version: '>= 1.0.0',
  default_options: [ 'warning_level=2', 'werror=false', ],
)

i18n = import('i18n')
gnome = import('gnome')
blueprint_compiler = find_program('blueprint-compiler', required: true)




subdir('data')
subdir('src')
subdir('po')

# Post install scripts are not needed for CLI tool.
