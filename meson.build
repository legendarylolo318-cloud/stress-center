project('stress-center',
        'rust', 'c',
          version: '1.2.0',
    meson_version: '>= 0.63.0',
  default_options: [ 'warning_level=2', 'werror=false', ],
)

i18n = import('i18n')
gnome = import('gnome')

c_compiler = meson.get_compiler('c')

if get_option('flatpak') == false
    proj_magpie = subproject('magpie', default_options: [
                                 'b_lto=' + get_option('b_lto').to_string(),
                             ])
endif

# stress-ng is a runtime-only dependency of the Stress page: it is invoked
# via PATH when a test is started, not linked against. Not required to
# configure or build; the Stress page shows an in-app message if it's
# missing at runtime instead of crashing.
stress_ng = find_program('stress-ng', required: false)
if not stress_ng.found()
    warning('stress-ng was not found. It is a runtime dependency of the Stress page ' +
            '(see stress_page/) and must be installed for that page to work.')
endif

subdir('data')
subdir('po')
subdir('resources')
subdir('src')

gnome.post_install(
     glib_compile_schemas: true,
    gtk_update_icon_cache: true,
  update_desktop_database: true,
)
