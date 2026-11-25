# Локальный Arch-пакет Muzon Native

Сборка текущего дерева исходников, без npm/Tauri и без загрузки старого коммита:

```bash
cd packaging/arch
makepkg -s
python ../aur-native/test_package.py ./muzon-native-0.2.0-1-x86_64.pkg.tar.zst
```

Установка — отдельное действие пользователя: `sudo pacman -U ./muzon-native-*.pkg.tar.zst`.
Пакет использует отдельный бинарь `muzon-native`, desktop ID `muzon-native` и профиль
`~/.local/share/muzon-native`; установленная Tauri-версия `muzon` остаётся доступной.
Встроенный Noto Sans распространяется с SIL OFL в каталоге лицензий пакета.

Этот PKGBUILD предназначен для локальной сборки. Для AUR нужен опубликованный
релиз нативных исходников: заменить `source=()` на URL архива конкретного тега,
указать проверенную SHA-256 и каталог распаковки вместо `_project_root`.
`packaging/aur-native/PKGBUILD` — самостоятельный AUR-рецепт `muzon-native-git`,
который получает опубликованные исходники из GitHub. Исторические патчи не применяются.

Выкладка и смена Git remote остаются под управлением владельца проекта.
