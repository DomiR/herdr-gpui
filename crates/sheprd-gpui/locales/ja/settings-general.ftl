# Settings > General, beyond the rows search finds (settings.ftl).

## Groups on the page.

settings-general-section-interface = インターフェイス
settings-general-section-features = 機能
settings-general-section-clipboard = クリップボード
settings-general-section-worktrees = ワークツリー
settings-general-section-browser-skill = ブラウザスキル
settings-general-section-startup = 起動
settings-general-section-updates = アップデート

## Rows.

# The language picker's first choice. $language is the name, in itself, of
# the language the system's preferences pick (English, עברית).
settings-general-language-system = システム（{ $language }）
# A tag the `language` setting names that has no messages. $tag is the BCP 47
# tag (fr-CA); $language names, in itself, the language shown instead.
settings-general-language-unavailable = { $tag }（{ $language } で表示）

settings-general-menu-bar = メニューバーを表示
settings-general-menu-bar-description = ウインドウ上部のメニューの列です。非表示の間は、F10 でウインドウの上にメニューが開き、「Toggle Menu Bar」で列が再表示されます。
settings-general-window-controls = ウインドウコントロールを表示
settings-general-window-controls-description = 最小化、最大化、閉じるボタンをメニューバーに表示します。メニューバーが非表示の間はタイトルバーに表示します。バーが二重にならないよう、ウインドウマネージャーでサーバーサイドデコレーションを無効にしてください。
settings-general-browser-skill-missing = インストールされていません。
settings-general-browser-skill-install = インストール
settings-general-browser-skill-remove = 削除
# Windows' own Startup switch has turned the entry off.
settings-general-launch-at-login-disabled = 「設定 > アプリ > スタートアップ」でオフになっています。そちらで再びオンにしてください。
settings-general-error-launch-at-login = ログイン時に起動：{ $error }
