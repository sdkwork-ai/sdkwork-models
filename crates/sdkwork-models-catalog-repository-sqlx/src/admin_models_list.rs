use sdkwork_models_contract_service::ListAdminAiModelsQuery;

pub const LIST_MODELS_BASE_WHERE_POSTGRES: &str = r#"
        WHERE (m.tenant_id IS NULL OR m.tenant_id = 0 OR m.tenant_id = $1)
          AND (m.organization_id IS NULL OR m.organization_id = 0 OR m.organization_id = $2)
          AND m.deleted_at IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM ai_model tenant_model
              WHERE tenant_model.tenant_id = $3
                AND tenant_model.organization_id = $4
                AND tenant_model.model = m.model
                AND tenant_model.id <> m.id
                AND tenant_model.deleted_at IS NULL
          )
          AND ($7::text IS NULL OR m.vendor_id::text = $7)
          AND (cardinality($8::text[]) = 0 OR m.vendor_code = ANY($8))
          AND (
              $9::text IS NULL
              OR m.display_name ILIKE $9
              OR m.model ILIKE $9
              OR m.vendor_code ILIKE $9
              OR COALESCE(m.catalog_key, '') ILIKE $9
          )
          AND (cardinality($10::int[]) = 0 OR m.capability = ANY($10))
          AND ($11::int IS NULL OR m.status = $11)
          AND (
              cardinality($12::text[]) = 0
              OR EXISTS (
                  SELECT 1
                  FROM jsonb_array_elements_text(COALESCE(m.modalities, '[]'::jsonb)) modality(value)
                  WHERE lower(modality.value) = ANY($12)
              )
          )
          AND (cardinality($13::int[]) = 0 OR m.release_stage = ANY($13))
          AND ($14::int IS NULL OR m.shelf_state = $14)
          AND ($15::int IS NULL OR m.routing_state = $15)
        ORDER BY
          COALESCE(m.rank_score, 0) DESC,
          CASE WHEN m.tenant_id = $5 AND m.organization_id = $6 THEN 0 ELSE 1 END,
          m.display_name ASC NULLS LAST,
          m.id ASC
        LIMIT $16 OFFSET $17
        "#;

pub const LIST_MODELS_COUNT_WHERE_POSTGRES: &str = r#"
        WHERE (m.tenant_id IS NULL OR m.tenant_id = 0 OR m.tenant_id = $1)
          AND (m.organization_id IS NULL OR m.organization_id = 0 OR m.organization_id = $2)
          AND m.deleted_at IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM ai_model tenant_model
              WHERE tenant_model.tenant_id = $3
                AND tenant_model.organization_id = $4
                AND tenant_model.model = m.model
                AND tenant_model.id <> m.id
                AND tenant_model.deleted_at IS NULL
          )
          AND ($5::text IS NULL OR m.vendor_id::text = $5)
          AND (cardinality($6::text[]) = 0 OR m.vendor_code = ANY($6))
          AND (
              $7::text IS NULL
              OR m.display_name ILIKE $7
              OR m.model ILIKE $7
              OR m.vendor_code ILIKE $7
              OR COALESCE(m.catalog_key, '') ILIKE $7
          )
          AND (cardinality($8::int[]) = 0 OR m.capability = ANY($8))
          AND ($9::int IS NULL OR m.status = $9)
          AND (
              cardinality($10::text[]) = 0
              OR EXISTS (
                  SELECT 1
                  FROM jsonb_array_elements_text(COALESCE(m.modalities, '[]'::jsonb)) modality(value)
                  WHERE lower(modality.value) = ANY($10)
              )
          )
          AND (cardinality($11::int[]) = 0 OR m.release_stage = ANY($11))
          AND ($12::int IS NULL OR m.shelf_state = $12)
          AND ($13::int IS NULL OR m.routing_state = $13)
        "#;

pub const LIST_MODELS_BASE_WHERE_SQLITE: &str = r#"
        WHERE (m.tenant_id IS NULL OR m.tenant_id = 0 OR m.tenant_id = ?)
          AND (m.organization_id IS NULL OR m.organization_id = 0 OR m.organization_id = ?)
          AND m.deleted_at IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM ai_model tenant_model
              WHERE tenant_model.tenant_id = ?
                AND tenant_model.organization_id = ?
                AND tenant_model.model = m.model
                AND tenant_model.id <> m.id
                AND tenant_model.deleted_at IS NULL
          )
          AND (? IS NULL OR m.vendor_id = ?)
          AND (
              ? IS NULL
              OR lower(m.display_name) LIKE lower(?)
              OR lower(m.model) LIKE lower(?)
              OR lower(m.vendor_code) LIKE lower(?)
              OR lower(COALESCE(m.catalog_key, '')) LIKE lower(?)
          )
          AND (? IS NULL OR m.status = ?)
          AND (? IS NULL OR m.shelf_state = ?)
          AND (? IS NULL OR m.routing_state = ?)
        "#;

pub const LIST_MODELS_ORDER_PAGE_SQLITE: &str = r#"
        ORDER BY
          CAST(COALESCE(m.rank_score, '0') AS REAL) DESC,
          CASE WHEN m.tenant_id = ? AND m.organization_id = ? THEN 0 ELSE 1 END,
          m.display_name ASC,
          m.id ASC
        LIMIT ? OFFSET ?
        "#;

pub const LIST_MODELS_COUNT_WHERE_SQLITE: &str = r#"
        WHERE (m.tenant_id IS NULL OR m.tenant_id = 0 OR m.tenant_id = ?)
          AND (m.organization_id IS NULL OR m.organization_id = 0 OR m.organization_id = ?)
          AND m.deleted_at IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM ai_model tenant_model
              WHERE tenant_model.tenant_id = ?
                AND tenant_model.organization_id = ?
                AND tenant_model.model = m.model
                AND tenant_model.id <> m.id
                AND tenant_model.deleted_at IS NULL
          )
          AND (? IS NULL OR m.vendor_id = ?)
          AND (
              ? IS NULL
              OR lower(m.display_name) LIKE lower(?)
              OR lower(m.model) LIKE lower(?)
              OR lower(m.vendor_code) LIKE lower(?)
              OR lower(COALESCE(m.catalog_key, '')) LIKE lower(?)
          )
          AND (? IS NULL OR m.status = ?)
          AND (? IS NULL OR m.shelf_state = ?)
          AND (? IS NULL OR m.routing_state = ?)
        "#;

pub fn normalized_search_pattern(query: &ListAdminAiModelsQuery) -> Option<String> {
    query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"))
}

pub fn normalized_vendor_codes(query: &ListAdminAiModelsQuery) -> Vec<String> {
    let mut values = query
        .vendor_codes
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

pub fn normalized_modalities(query: &ListAdminAiModelsQuery) -> Vec<String> {
    let mut values = query
        .modalities
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

pub fn normalized_release_stages(query: &ListAdminAiModelsQuery) -> Vec<i32> {
    let mut values = query.release_stages.clone();
    values.sort_unstable();
    values.dedup();
    values
}

pub fn optional_non_empty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn capability_codes_from_model_types(value: Option<&str>) -> Vec<i32> {
    value
        .map(|raw| {
            raw.split(',')
                .filter_map(|segment| {
                    let trimmed = segment.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(
                            crate::model_modality::code_from_text(Some(trimmed))
                                .map(|value| value as i32)
                                .unwrap_or_else(|| {
                                    crate::model_modality::model_type_capability_code(trimmed)
                                }),
                        )
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_codes_from_model_types_parses_admin_labels() {
        let codes = capability_codes_from_model_types(Some("Chat,Image,Embedding"));
        assert_eq!(codes, vec![1, 2, 6]);
        let lowercase_codes = capability_codes_from_model_types(Some("chat,image,embedding"));
        assert_eq!(lowercase_codes, vec![1, 2, 6]);
    }

    #[test]
    fn normalized_vendor_codes_trims_sorts_and_deduplicates() {
        let query = ListAdminAiModelsQuery {
            subject: sdkwork_models_contract_service::AdminModelSubject {
                tenant_id: 1,
                organization_id: 0,
                operator_id: 1,
                operator_type: 1,
            },
            vendor_id: None,
            vendor_codes: vec![
                " OpenAI ".to_owned(),
                "anthropic".to_owned(),
                "openai".to_owned(),
            ],
            q: None,
            model_types: None,
            status: None,
            release_stages: Vec::new(),
            shelf_state: None,
            routing_state: None,
            modalities: Vec::new(),
            page_size: None,
            offset: None,
        };

        assert_eq!(
            normalized_vendor_codes(&query),
            vec!["anthropic".to_owned(), "openai".to_owned()]
        );
    }

    #[test]
    fn normalized_modalities_trims_sorts_and_deduplicates() {
        let query = ListAdminAiModelsQuery {
            subject: sdkwork_models_contract_service::AdminModelSubject {
                tenant_id: 1,
                organization_id: 0,
                operator_id: 1,
                operator_type: 1,
            },
            vendor_id: None,
            vendor_codes: Vec::new(),
            q: None,
            model_types: None,
            status: Some("active".to_owned()),
            release_stages: vec![2, 1, 2],
            shelf_state: Some(1),
            routing_state: Some(1),
            modalities: vec![" Text ".to_owned(), "image".to_owned(), "text".to_owned()],
            page_size: None,
            offset: None,
        };

        assert_eq!(
            normalized_modalities(&query),
            vec!["image".to_owned(), "text".to_owned()]
        );
        assert_eq!(normalized_release_stages(&query), vec![1, 2]);
    }

    #[test]
    fn public_lifecycle_filters_precede_ranked_page_selection() {
        let lifecycle = [
            "m.status = $11",
            "m.release_stage = ANY($13)",
            "m.shelf_state = $14",
            "m.routing_state = $15",
        ];
        let order_position = LIST_MODELS_BASE_WHERE_POSTGRES
            .find("ORDER BY")
            .expect("rank ordering");
        let limit_position = LIST_MODELS_BASE_WHERE_POSTGRES
            .find("LIMIT $16 OFFSET $17")
            .expect("SQL pagination");
        for predicate in lifecycle {
            let position = LIST_MODELS_BASE_WHERE_POSTGRES
                .find(predicate)
                .unwrap_or_else(|| panic!("missing lifecycle predicate: {predicate}"));
            assert!(position < order_position);
            assert!(position < limit_position);
        }
        assert!(LIST_MODELS_BASE_WHERE_POSTGRES.contains("COALESCE(m.rank_score, 0) DESC"));
        assert!(LIST_MODELS_ORDER_PAGE_SQLITE
            .contains("CAST(COALESCE(m.rank_score, '0') AS REAL) DESC"));
    }
}
