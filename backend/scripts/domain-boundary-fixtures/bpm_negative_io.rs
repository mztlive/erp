fn forbidden_id_generation() {
    let _ = Uuid::new_v4();
    let _ = Uuid::new();
    let _ = uuid::Uuid::new(0);
    let _ = nanoid!();
    let _ = IdGenerator::next();
    let _ = Utc::now();
}
