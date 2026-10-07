use sqlx::{Postgres, QueryBuilder};

use crate::domains::profiles::service::{content_rating_rank, is_kids};
use crate::domains::profiles::types::ProfileScope;

pub(crate) async fn load_browse_scope(
    pool: &sqlx::PgPool,
    user: &crate::extractors::AuthenticatedUser,
) -> Result<ProfileScope, crate::domains::profiles::ProfilesError> {
    if user.profile_selection_required {
        return Err(crate::domains::profiles::ProfilesError::SelectionRequired);
    }
    crate::domains::profiles::service::load_profile_scope(
        pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await
}

pub(crate) fn scoped_media_query(scope: &ProfileScope) -> QueryBuilder<Postgres> {
    let mut sql = QueryBuilder::new(
        "WITH allowed_media AS NOT MATERIALIZED (SELECT scoped.* FROM media_items scoped JOIN libraries library ON library.id = scoped.library_id WHERE library.deleted_at IS NULL",
    );
    push_library_scope(&mut sql, scope, "scoped.library_id");
    if is_kids(scope) {
        sql.push(" AND CASE split_part(split_part(upper(btrim(scoped.content_rating, ")
            .push_bind(" \t\n\r\u{000b}\u{000c}\u{0085}\u{00a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}".to_string())
            .push(")), ' ', 1), ':', 1)");
        for (aliases, rank) in [
            (&["TVY", "TV-Y"][..], 0),
            (&["TVY7", "TV-Y7", "TV-Y7-FV"][..], 1),
            (&["G"][..], 2),
            (&["TVG", "TV-G"][..], 3),
            (&["PG"][..], 4),
            (&["TVPG", "TV-PG"][..], 5),
            (&["PG13", "PG-13"][..], 6),
            (&["TV14", "TV-14"][..], 7),
            (&["R"][..], 8),
            (&["TVMA", "TV-MA"][..], 9),
            (&["NC17", "NC-17"][..], 10),
        ] {
            for alias in aliases {
                sql.push(" WHEN ")
                    .push_bind(alias.to_string())
                    .push(" THEN ")
                    .push(rank.to_string());
            }
        }
        sql.push(" ELSE NULL END <= ").push_bind(
            content_rating_rank(&scope.max_content_rating)
                .map(i32::from)
                .unwrap_or(-1),
        );
    }
    sql.push(") ");
    sql
}

pub(crate) fn scoped_libraries_query(scope: &ProfileScope) -> QueryBuilder<Postgres> {
    let mut sql = QueryBuilder::new(
        "WITH allowed_libraries AS NOT MATERIALIZED (SELECT library.id, library.name, library.media_type FROM libraries library WHERE library.deleted_at IS NULL",
    );
    push_library_scope(&mut sql, scope, "library.id");
    sql.push(") ");
    sql
}

pub(crate) fn push_library_scope(
    sql: &mut QueryBuilder<Postgres>,
    scope: &ProfileScope,
    column: &str,
) {
    sql.push(" AND (")
        .push_bind(scope.has_all_library_access)
        .push(" OR ")
        .push(column)
        .push(" = ANY(")
        .push_bind(scope.user_library_ids.clone())
        .push("::uuid[]))");
    if is_kids(scope) {
        sql.push(" AND ")
            .push(column)
            .push(" = ANY(")
            .push_bind(scope.library_ids.clone())
            .push("::uuid[])");
    }
}
