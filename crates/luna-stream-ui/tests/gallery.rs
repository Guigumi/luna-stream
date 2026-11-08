use luna_stream_ui::gallery::{
    GalleryFilter, GalleryManager, GallerySortField, MediaKind, SortOrder, format_duration,
    format_size_mb,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_unification_without_duplicates_and_name_preservation() {
    let tmp = tempdir().unwrap();
    let recordings_dir = tmp.path().join("Videos").join("Luna");
    let clips_dir = recordings_dir.join("Clips");
    let cache_dir = tmp.path().join("cache");

    fs::create_dir_all(&clips_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();

    // Cria gravação no diretório pai
    let rec_file = recordings_dir.join("2026-11-08_14-30-00_Counter_Strike_2.mkv");
    fs::write(&rec_file, b"recording-data-bytes").unwrap();

    // Cria clipe no subdiretório Clips
    let clip_file = clips_dir.join("Clip_2026-11-08_14-35-00_Counter_Strike_2_3.mp4");
    fs::write(&clip_file, b"clip-data-bytes-collision").unwrap();

    // Cria arquivo UTF-8 multi-byte
    let utf8_file = clips_dir.join("Clip_2026-11-08_15-00-00_ブラッドボーン.mp4");
    fs::write(&utf8_file, b"utf8-japanese").unwrap();

    let mut gallery = GalleryManager::new(recordings_dir.clone(), clips_dir.clone(), cache_dir);
    gallery.refresh().unwrap();

    let items = gallery.visible_items();
    // Exatamente 3 itens (a pasta Clips não foi duplicada como gravação)
    assert_eq!(items.len(), 3);

    let rec_item = items.iter().find(|i| i.path == rec_file).unwrap();
    assert_eq!(rec_item.kind, MediaKind::Recording);
    assert_eq!(rec_item.format, "mkv");
    assert_eq!(rec_item.title, "Counter Strike 2");
    assert_eq!(rec_item.year, 2026);
    assert_eq!(rec_item.month, 11);
    assert_eq!(rec_item.day, 8);

    let clip_item = items.iter().find(|i| i.path == clip_file).unwrap();
    assert_eq!(clip_item.kind, MediaKind::Clip);
    assert_eq!(clip_item.format, "mp4");
    assert_eq!(clip_item.title, "Counter Strike 2"); // _3 removido, preservando o número 2 do nome

    let utf8_item = items.iter().find(|i| i.path == utf8_file).unwrap();
    assert_eq!(utf8_item.title, "ブラッドボーン");
}

#[test]
fn test_filters_facets_and_sorting() {
    let tmp = tempdir().unwrap();
    let recordings_dir = tmp.path().join("Recordings");
    let clips_dir = tmp.path().join("Clips");
    let cache_dir = tmp.path().join("cache");

    fs::create_dir_all(&recordings_dir).unwrap();
    fs::create_dir_all(&clips_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();

    let f1 = clips_dir.join("Clip_2026-10-01_10-00-00_GameA.mp4");
    fs::write(&f1, vec![0u8; 1000]).unwrap(); // 1 KB

    let f2 = clips_dir.join("Clip_2026-11-08_12-00-00_GameB.mp4");
    fs::write(&f2, vec![0u8; 5000]).unwrap(); // 5 KB

    let f3 = recordings_dir.join("2025-05-15_18-00-00_GameC.mkv");
    fs::write(&f3, vec![0u8; 3000]).unwrap(); // 3 KB

    let mut gallery = GalleryManager::new(recordings_dir, clips_dir, cache_dir);
    gallery.refresh().unwrap();

    // Mock probe duration: f1 = 10s (10000ms), f2 = 60s (60000ms), f3 = 30s (30000ms)
    gallery.record_probe_result(&f1, 10_000, true);
    gallery.record_probe_result(&f2, 60_000, true);
    gallery.record_probe_result(&f3, 30_000, true);

    // Facetas
    let facets = gallery.facets();
    assert_eq!(facets.years[0], "Todos");
    assert!(facets.years.contains(&"2026".to_string()));
    assert!(facets.years.contains(&"2025".to_string()));

    // Filtro por Ano: 2026 (f1 e f2)
    gallery.set_filter(GalleryFilter {
        year: Some(2026),
        ..Default::default()
    });
    assert_eq!(gallery.visible_items().len(), 2);

    // Filtro por Tipo: apenas Gravações (f3 é de 2025, então ano 2026 + Recording -> 0)
    gallery.set_filter(GalleryFilter {
        kind: Some(MediaKind::Recording),
        ..Default::default()
    });
    let recs = gallery.visible_items();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].path, f3);

    // Ordenação por Tamanho (Crescente)
    gallery.set_filter(GalleryFilter::default());
    gallery.set_sort(GallerySortField::Size, SortOrder::Asc);
    let by_size = gallery.visible_items();
    assert_eq!(by_size[0].path, f1); // 1000
    assert_eq!(by_size[1].path, f3); // 3000
    assert_eq!(by_size[2].path, f2); // 5000

    // Ordenação por Duração (Decrescente)
    gallery.set_sort(GallerySortField::Duration, SortOrder::Desc);
    let by_duration = gallery.visible_items();
    assert_eq!(by_duration[0].path, f2); // 60s
    assert_eq!(by_duration[1].path, f3); // 30s
    assert_eq!(by_duration[2].path, f1); // 10s
}

#[test]
fn test_cache_index_persistence_and_invalidation() {
    let tmp = tempdir().unwrap();
    let recordings_dir = tmp.path().join("Recordings");
    let clips_dir = tmp.path().join("Clips");
    let cache_dir = tmp.path().join("cache");

    fs::create_dir_all(&recordings_dir).unwrap();
    fs::create_dir_all(&clips_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();

    let clip_file = clips_dir.join("Clip_2026-11-08_14-30-00_TestGame.mp4");
    fs::write(&clip_file, b"initial-content-bytes").unwrap();

    let mut gallery1 =
        GalleryManager::new(recordings_dir.clone(), clips_dir.clone(), cache_dir.clone());
    gallery1.refresh().unwrap();

    let unprobed = gallery1.next_unprobed();
    assert!(unprobed.is_some());
    let (v_path, thumb_target) = unprobed.unwrap();
    assert_eq!(v_path, clip_file);

    // Simula probe gravando thumb e duração
    fs::write(&thumb_target, b"dummy-png").unwrap();
    gallery1.record_probe_result(&clip_file, 42_000, true);

    // Segunda instância lê o cache persistido
    let mut gallery2 =
        GalleryManager::new(recordings_dir.clone(), clips_dir.clone(), cache_dir.clone());
    gallery2.refresh().unwrap();
    let items = gallery2.visible_items();
    assert_eq!(items[0].duration_ms, 42_000);
    assert!(items[0].probed);
    assert_eq!(items[0].thumb_path, Some(thumb_target.clone()));
    assert_eq!(gallery2.next_unprobed(), None);

    // Modifica o arquivo (altera tamanho) -> deve invalidar o cache
    fs::write(&clip_file, b"modified-content-bytes-longer-now").unwrap();
    gallery2.refresh().unwrap();
    let items_after_change = gallery2.visible_items();
    assert_eq!(items_after_change[0].duration_ms, 0);
    assert!(!items_after_change[0].probed);
}

#[test]
fn test_two_step_inline_delete_confirmation_safety() {
    let tmp = tempdir().unwrap();
    let recordings_dir = tmp.path().join("Recordings");
    let clips_dir = tmp.path().join("Clips");
    let cache_dir = tmp.path().join("cache");

    fs::create_dir_all(&recordings_dir).unwrap();
    fs::create_dir_all(&clips_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();

    let f_a = clips_dir.join("Clip_2026-11-08_10-00-00_ItemA.mp4");
    let f_b = clips_dir.join("Clip_2026-11-08_11-00-00_ItemB.mp4");
    fs::write(&f_a, b"video-a").unwrap();
    fs::write(&f_b, b"video-b").unwrap();

    let mut gallery = GalleryManager::new(recordings_dir, clips_dir, cache_dir);
    gallery.refresh().unwrap();

    // 1. Chamar confirm_delete(&f_a) sem request_delete(&f_a) NUNCA deleta
    let deleted = gallery.confirm_delete(&f_a).unwrap();
    assert!(!deleted);
    assert!(f_a.exists(), "f_a deve continuar no disco!");

    // 2. request_delete(&f_a) ativa f_a
    gallery.request_delete(&f_a);
    assert_eq!(gallery.confirming_delete_path(), Some(f_a.as_path()));

    // 3. Alterar filtro limpa o estado de confirmação para evitar deletar item errado
    gallery.set_filter(GalleryFilter {
        kind: Some(MediaKind::Clip),
        ..Default::default()
    });
    assert_eq!(gallery.confirming_delete_path(), None);
    let deleted = gallery.confirm_delete(&f_a).unwrap();
    assert!(!deleted);
    assert!(f_a.exists());

    // 4. request_delete(&f_a) e em seguida request_delete(&f_b) desativa f_a e ativa f_b
    gallery.request_delete(&f_a);
    gallery.request_delete(&f_b);
    assert_eq!(gallery.confirming_delete_path(), Some(f_b.as_path()));

    // Tentar confirmar f_a deve falhar
    let deleted_a = gallery.confirm_delete(&f_a).unwrap();
    assert!(!deleted_a);
    assert!(f_a.exists());

    // 5. Confirmar f_b deleta com sucesso do disco e da memória
    let deleted_b = gallery.confirm_delete(&f_b).unwrap();
    assert!(deleted_b);
    assert!(!f_b.exists(), "f_b deve ter sido excluído do disco!");
    assert_eq!(gallery.visible_items().len(), 1);
    assert_eq!(gallery.visible_items()[0].path, f_a);
}

#[test]
fn test_duration_and_size_formatters() {
    assert_eq!(format_duration(0, false), "--:--");
    assert_eq!(format_duration(0, true), "--:--");
    assert_eq!(format_duration(15_000, true), "00:15");
    assert_eq!(format_duration(65_000, true), "01:05");
    assert_eq!(format_duration(3_665_000, true), "01:01:05");

    assert_eq!(format_size_mb(500), "500 B");
    assert_eq!(format_size_mb(1024 * 200), "200 KB");
    assert_eq!(format_size_mb(1024 * 1024 * 15), "15,0 MB");
}
