-- ============================================================================
-- FILE: 0014_index_homework_files_storage_key.sql
-- PURPOSE: Lookup of homework file records by storage object.
-- DEPENDENCIES: [0004_create_homework.sql]
-- ============================================================================

-- A file attached to both lessons of a double lesson is one object in storage with a
-- record in each homework. Deleting a record must find its twins and delete the object
-- only when no record is left.

-- ИНДЕКС: idx_homework_files_storage_key
-- Цель: ускоряет поиск записей одного объекта при удалении файла или ДЗ.
-- Тип: обычный B-tree, запрос по точному совпадению storage_key.
CREATE INDEX idx_homework_files_storage_key ON homework_files (storage_key);
