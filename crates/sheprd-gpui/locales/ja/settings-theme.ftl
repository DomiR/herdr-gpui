# Settings > Theme: appearance, the theme browser, the window backdrop, and
# terminal padding. Row titles are in settings.ftl; theme names are data.

settings-theme-section-mode = モード
settings-theme-section-themes = テーマ
settings-theme-section-window = ウインドウ
settings-theme-section-padding = ターミナルの余白
settings-theme-light = ライト
settings-theme-dark = ダーク
settings-theme-system = システム
settings-theme-mode-hint = ライト、ダーク、またはシステムに合わせます。変更するには下のテーマを選択してください。
# $light and $dark are theme names.
settings-theme-side-hint = 下のテーマ一覧で、ライトとダークのどちらを変更するかを選びます。ライト：{ $light } · ダーク：{ $dark }
# Shown for the theme value that uses Herdr's own theme.
settings-theme-follow-herdr = Herdr に従う
settings-theme-library-all = すべて
settings-theme-library-ghostty = Ghostty
settings-theme-library-herdr = Herdr
settings-theme-library-read-only = このプラットフォームでは Herdr のテーマは読み取り専用です。
settings-theme-read-only = このプラットフォームでは Herdr のテーマは読み取り専用です。代わりに「このアプリ」か「Herdr に従う」を選択してください。
settings-theme-open-file = テーマファイルを開く
settings-theme-search-placeholder = テーマを検索…
settings-theme-count = { $total } 件中 { $shown } 件のテーマ
settings-theme-count-discovering = { $total } 件中 { $shown } 件のテーマ / インストール済みのテーマを検出中…
settings-theme-empty = 一致するテーマがありません。検索語を短くしてみてください。

## Where a theme card's theme comes from, and whether it is drafted or saved.

settings-theme-kind-herdr = Herdr
settings-theme-kind-built-in = 組み込み
settings-theme-kind-file = ファイル
settings-theme-kind-ghostty = Ghostty
# $source is one of the settings-theme-kind- messages.
settings-theme-card-draft = { $source } / 下書き
settings-theme-card-saved = { $source } / 保存済み
settings-theme-thumbnail-unavailable = 利用不可
settings-theme-thumbnail-loading = 読み込み中…
settings-theme-catalog-error = テーマ一覧の検出に失敗しました：{ $error }。組み込みテーマは引き続き使用できます。
settings-theme-retry-catalog = 検出を再試行
settings-theme-preview-error = プレビューを表示できません：{ $error }。最後に有効だったプレビューを表示したままにします。
settings-theme-retry-preview = プレビューを再試行
settings-theme-error-load = テーマを読み込めませんでした：{ $error }

## The window backdrop.

settings-theme-backdrop-solid = 単色
settings-theme-backdrop-translucent = 半透明
settings-theme-backdrop-picture = 画像
settings-theme-backdrop-solid-hint = ウインドウは従来どおりに描画されます。
settings-theme-backdrop-translucent-hint = プログラムが指定した色、ポップアップ、ダイアログは不透明のままです。
settings-theme-backdrop-picture-hint = ウインドウは画像の上で不透明のままです。不透明度を下げるほど画像が透けて見えます。
settings-theme-backdrop-blur = 背後をぼかす
settings-theme-backdrop-mica = Mica
settings-theme-backdrop-mica-hint = Mica は背後のウインドウではなく壁紙の色でウインドウを色付けします。
settings-theme-backdrop-change = 変更
settings-theme-backdrop-remove = 削除
# The button of the file chooser that picks a picture.
settings-theme-backdrop-choose = 選択
settings-theme-backdrop-opacity = 背景の不透明度
settings-theme-backdrop-percent = { $percent }%

## Terminal padding.

settings-theme-padding-left-hint = サイドバー側の余白です。未設定の間は、非推奨の sidebar_gap が代わりに使われます。
