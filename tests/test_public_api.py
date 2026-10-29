class TestCreateMathVerifyProjectionTopLevelExport:
    def test_create_math_verify_projection_importable_from_top_level(self):
        from its_hub import create_math_verify_projection  # noqa: F401

    def test_top_level_all_includes_projection(self):
        import its_hub

        assert "create_math_verify_projection" in its_hub.__all__

    def test_top_level_export_is_same_object_as_projections_module(self):
        import its_hub
        from its_hub.core.algorithms.projections import (
            create_math_verify_projection,
        )

        assert its_hub.create_math_verify_projection is create_math_verify_projection
