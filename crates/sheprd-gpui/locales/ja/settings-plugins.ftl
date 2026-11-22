# Settings > Plugins: the values hosts' plugins report, a switch per value,
# and a preview of the sidebar rows they make.

settings-plugins-search-placeholder = プラグインの値を検索…
settings-plugins-config-note = 表示する値は Herdr の構成の [ui.sidebar] に独立した行として追加されるため、ターミナルアプリにも表示されます。
# What Herdr's endpoint cannot do yet, so the page never implies otherwise.
settings-plugins-limits = Herdr ではまだ、このアプリからプラグインの一覧表示、有効化・無効化、ログの閲覧はできません。ホストで `herdr plugin` を使用してください。ここからは何もインストールされません。
settings-plugins-no-window = プラグインが報告する内容を確認するには、セッションのウインドウを開いてください。Sheprd の設定は引き続き利用できます。
settings-plugins-read-only = このプラットフォームでは、Herdr の設定は読み取り専用です。
settings-plugins-unavailable = Herdr の設定を利用できません。再読み込みして再試行してください。
settings-plugins-scope-agents = エージェント
settings-plugins-scope-spaces = ワークスペース
# $scope is Agents or Workspaces, $sample a value a plugin reported, and
# $hosts the hosts reporting it.
settings-plugins-detail = { $scope } · 「{ $sample }」 · { $hosts }
settings-plugins-detail-missing = { $scope } · 未報告
settings-plugins-status-labels = ステータスラベル
settings-plugins-status-labels-detail = エージェント · { $count } 件が独自のステータス名を使用（例：「{ $sample }」）
settings-plugins-none = 接続中のホストで、カスタム値を報告しているプラグインやフックはまだありません。
settings-plugins-no-match = 一致する値はありません。
settings-plugins-native-rows = このアプリはサイドバーの行を独自に描画するため（[usage] inline = false）、これらのスイッチはターミナルアプリにだけ反映されます。
settings-plugins-use-herdr-rows = このアプリで Herdr のサイドバー行を使用
settings-plugins-default-rows = Herdr の行がデフォルトのままの間、サイドバーはこのアプリ独自の行を使います。値を表示すると、Herdr で構成された行に、あなたの行スタイルで切り替わります。見た目はプレビューで確認できます。
settings-plugins-preview = プレビュー
