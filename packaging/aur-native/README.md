# AUR PKGBUILD: muzon-native-git

Самостоятельный VCS-рецепт. Для AUR нужны только `PKGBUILD` и `.SRCINFO`;
ссылок на соседний локальный рецепт нет. makepkg получает исходники из GitHub,
вычисляет версию из Cargo.toml и Git revision и собирает нативный workspace.
Команда и desktop entry — `muzon-native`, зависимости npm/Tauri отсутствуют.

```bash
cd packaging/aur-native
makepkg -s
python test_package.py ./muzon-native-git-*.pkg.tar.zst
```

Сначала нативная миграция должна попасть в опубликованный upstream. Пока там
отсутствуют корневой workspace/lockfile и `crates/muzon-native`, prepare()
завершит сборку с понятной ошибкой. Локальные незакоммиченные файлы не используются.
`pkgver=0.2.0.r0` — начальное значение; makepkg вычисляет версию через pkgver().

Пакет предоставляет `muzon-native` и конфликтует с ним; старая Tauri-версия
`muzon` может оставаться установленной. Профиль — `~/.local/share/muzon-native`.
Локальный рецепт текущего дерева остаётся в `packaging/arch`.

Перед AUR-публикацией пользователь пушит нативные исходники, собирает пакет,
обновляет `.SRCINFO` (`makepkg --printsrcinfo > .SRCINFO`) и переносит эти два
файла в AUR checkout. Автоматической Git/AUR-публикации нет.
Исторические native-app-id.patch и native-scrollbars.patch не применяются.
