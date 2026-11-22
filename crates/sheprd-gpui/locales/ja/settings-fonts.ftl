# Settings > Fonts: the faces, the font picker, and the specimen. Each
# face's title is in settings.ftl; font family names are data.

settings-fonts-section-faces = 表示箇所
settings-fonts-use-one = すべてに同じファミリーを使用…
# The family `.SystemUIFont`, the platform's interface font.
settings-fonts-system-font = システムフォント
settings-fonts-size = { $size } px

## The specimen of the face last picked, set in that face.

settings-fonts-specimen-terminal = 見本 · ターミナル
settings-fonts-specimen-sidebar = 見本 · サイドバー
settings-fonts-specimen-tabs = 見本 · タブ
settings-fonts-specimen-ui = 見本 · インターフェイス
# Sample text shaped like the sidebar, a line each: a workspace, then two
# agents and their status.
settings-fonts-sample-sidebar-1 = sheprd / ワークスペース
settings-fonts-sample-sidebar-2 = {"  "}claude  作業中
settings-fonts-sample-sidebar-3 = {"  "}codex   レビュー待ち
# Sample text shaped like two tab strips, a line each.
settings-fonts-sample-tabs-1 = main.rs   エージェント / レビュー
settings-fonts-sample-tabs-2 = 変更   ターミナル   プレビュー
settings-fonts-sample-ui-1 = 集中して作業できる場所を。
settings-fonts-sample-ui-2 = 好みに合わせた、自分だけのワークスペース。

## The font picker. $role is which face it sets: all, terminal, sidebar,
## tabs, or ui.

settings-fonts-picker-title =
    フォントファミリー / { $role ->
        [all] すべての表示箇所
        [terminal] ターミナル
        [sidebar] サイドバー
        [tabs] タブ
       *[ui] インターフェイス
    }
settings-fonts-picker-default =
    プラットフォームのデフォルトのファミリーを使用 / { $role ->
        [all] すべての表示箇所
        [terminal] ターミナル
        [sidebar] サイドバー
        [tabs] タブ
       *[ui] インターフェイス
    }
settings-fonts-picker-close = 閉じる
settings-fonts-loading = インストール済みのフォントを読み込み中…
settings-fonts-count = { $total } 件中 { $shown } 件のインストール済みファミリー
settings-fonts-no-match = 一致するフォントがありません。
settings-fonts-search-placeholder = インストール済みのフォントを検索…
