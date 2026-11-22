# Settings > Cloud devices > Azure. Size, region, group, and machine names
# come from Azure and the user and stay as they are. $kind is `vm` or
# `sandbox`, for what kind of machine a device is.

settings-cloud-azure-note = 1 つの Azure サブスクリプション内の VM と Container Apps サンドボックスをデバイスとして使います。VM には SSH で、サンドボックスには Azure データプレーン経由で接続します。

## The account's fields, each a label and its placeholder.

settings-cloud-azure-subscription = サブスクリプション ID
settings-cloud-azure-subscription-placeholder = Azure サブスクリプションの UUID
settings-cloud-azure-group = リソースグループ
settings-cloud-azure-group-placeholder = デフォルト：herdr-devices
settings-cloud-azure-location = 場所
settings-cloud-azure-location-placeholder = デフォルト：centralus
settings-cloud-azure-vm-size = VM サイズ
settings-cloud-azure-vm-size-placeholder = デフォルト：Standard_D2plds_v6
settings-cloud-azure-auto-stop = 自動停止
# "never" and the times are what the field takes; keep them as they are.
settings-cloud-azure-auto-stop-placeholder = never、またはアイドル状態の VM を削除するまでの時間（例：30m、1h、2h）
settings-cloud-azure-tenant = テナント ID
settings-cloud-azure-tenant-placeholder = サービスプリンシパルのテナント UUID（任意）
settings-cloud-azure-sandbox-group = サンドボックスグループ
settings-cloud-azure-sandbox-group-placeholder = デフォルト：herdr-sandboxes
settings-cloud-azure-sandbox-region = サンドボックスのリージョン
settings-cloud-azure-sandbox-region-placeholder = デフォルト：「場所」と同じ
settings-cloud-azure-sandbox-image = サンドボックスのイメージ
settings-cloud-azure-sandbox-image-placeholder = パブリックディスクイメージ。デフォルト：ubuntu
settings-cloud-azure-client-id = クライアント ID
settings-cloud-azure-client-id-placeholder = サービスプリンシパルのアプリケーション UUID（任意）
settings-cloud-azure-secret = クライアントシークレット
settings-cloud-azure-secret-placeholder = クライアントシークレット（マスク表示）

## How the account signs in.

settings-cloud-azure-cli-detected = Azure CLI（`az`）を検出しました。サブスクリプション ID を入力するか、「保存」をクリックすると現在の CLI アカウントを使用します。
settings-cloud-azure-not-configured = 未設定です。サブスクリプション ID と認証情報を入力して保存してください。
settings-cloud-azure-signed-in-cli = 現在の Azure CLI（`az`）アカウントで認証済みです。
settings-cloud-azure-signed-in-secret = 保存済みのサービスプリンシパルのクライアントシークレットで認証済みです。
settings-cloud-azure-signed-in-environment = HERDR_AZURE_CLIENT_SECRET で認証済みです。
settings-cloud-azure-credential-missing = サブスクリプションは設定済みです。接続するには、クライアントシークレットを追加するか、`az` CLI にログインしてください。

## New VM and New sandbox.

settings-cloud-azure-new-vm = 新規 VM
settings-cloud-azure-create-vm = VM を作成
settings-cloud-azure-creating-vm = Azure VM { $name } を作成しています。進行状況はデバイスピッカーに表示されます。
settings-cloud-azure-no-window-vm = VM を作成するには、Sheprd のメインウインドウを開いてください。
settings-cloud-azure-new-sandbox = 新規サンドボックス
settings-cloud-azure-create-sandbox = サンドボックスを作成
settings-cloud-azure-creating-sandbox = Azure サンドボックス { $name } を作成しています。進行状況はデバイスピッカーに表示されます。
settings-cloud-azure-no-window-sandbox = サンドボックスを作成するには、Sheprd のメインウインドウを開いてください。
settings-cloud-azure-vm-sizes-note = 新しい VM は OS ディスクを VM のローカルストレージに置くため、ディスクの料金はかかりません。また、自動停止はアイドル状態の VM を削除するので、作業は Git に残してください。Azure がサブスクリプションに Spot を提供していれば Spot で実行され、Azure が容量を必要とすると Spot VM が削除されることがあります。それ以外の場合は通常料金で課金されます。{ $location } での Linux の定価（米ドル、現時点）です。Spot 料金は変動します。
settings-cloud-azure-sizes-unknown = スペックと料金は、アカウントを保存して接続できるようになると表示されます。
settings-cloud-azure-size-no-local-disk = ローカルストレージなし：Standard SSD ディスク、削除するまで課金
settings-cloud-azure-size-not-offered = { $location } では提供されていません
# Hourly and monthly prices, as Azure lists them.
settings-cloud-azure-price-spot-regular = Spot { $spot } · 通常 { $regular }
settings-cloud-azure-price-spot = Spot { $spot }
settings-cloud-azure-price-regular = 通常 { $regular }
settings-cloud-azure-size-other = その他
settings-cloud-azure-size-other-detail = リージョンで提供されている任意のサイズ
settings-cloud-azure-size-use = このサイズを使用
settings-cloud-azure-sandbox-sizes-note = 実行中は { $location } の Container Apps の料金で秒単位で課金されます。一時停止中のサンドボックスには CPU とメモリの料金はかかりません。
settings-cloud-azure-sandbox-sizes-unknown = vCPU あたり、メモリは 2 GiB、ディスクは 20 GiB です。
# $size is the size's vCPUs and memory, $disk its disk in GiB, $price what
# it costs an hour.
settings-cloud-azure-sandbox-size = { $size } · ディスク { $disk } GiB
settings-cloud-azure-sandbox-price = { $size } · ディスク { $disk } GiB · 実行中 { $price }
settings-cloud-azure-vcpu = { $cores } vCPU

## Azure devices.

settings-cloud-azure-devices = Azure デバイス
settings-cloud-azure-remove-note = デバイスを削除すると、ディスクと IP ごと Azure 上から削除されます。横に表示される「Sheprd からのみ削除」を使うと Azure 上に残り、課金が続きます。
settings-cloud-azure-device = { $kind ->
        [vm] { $group } 内の VM
       *[sandbox] { $group } 内のサンドボックス
    }
settings-cloud-azure-device-size = { $kind ->
        [vm] VM · { $size } · { $group } 内
       *[sandbox] サンドボックス · { $size } · { $group } 内
    }
settings-cloud-azure-delete = { $kind ->
        [vm] VM を削除
       *[sandbox] サンドボックスを削除
    }
settings-cloud-azure-delete-vm-warning = 削除すると、VM { $name } がディスク、パブリック IP、ネットワークとともに、ここだけでなく Azure 上からも削除されます。ファイルは保持されず、元に戻すことはできません。「Sheprd からのみ削除」では Azure 上に残り、課金が続きます。
settings-cloud-azure-delete-sandbox-warning = 削除すると、サンドボックス { $name } がここだけでなく Azure 上からも削除されます。ファイルは保持されず、元に戻すことはできません。「Sheprd からのみ削除」では Azure 上に残り、課金が続きます。
settings-cloud-azure-destroy-contacting = Azure に接続しています…
settings-cloud-azure-destroy-vm = VM とディスクを削除しています…
settings-cloud-azure-destroy-leftovers = ネットワークと IP を削除しています…
settings-cloud-azure-destroy-sandbox = サンドボックスを削除しています…
