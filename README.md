# Cosmic Samba Manager

Нативное приложение для рабочего стола **Cosmic** (System76) на Rust.

Управляет Samba-сервером и позволяет удобно добавлять и настраивать шары.

## Возможности

- Старт / стоп / статус `smbd`
- Добавление и удаление шар
- Права доступа (Read only / Guest)
- Комментарии к шарам
- Управление пользователями Samba
- Выбор папки через системный диалог
- Красивые карточки в стиле Cosmic

## Сборка

Требуется:
- Rust (stable)
- Samba (`smbd`, `smbpasswd`, `pdbedit`)
- `just` (опционально)
- `zenity` (для выбора папки)

```bash
# С just
just build-release

# Или обычным cargo
cargo build --release
```

## Установка

```bash
just install
```

Или вручную:

```bash
sudo install -Dm755 target/release/cosmic-samba /usr/bin/cosmic-samba
sudo install -Dm644 data/com.krmobdev.CosmicSamba.desktop /usr/share/applications/
```

## Запуск

```bash
cosmic-samba
# или
just run
```

Сделано с любовью для Рустама ❤️
