#include "pinned_test_fonts.h"

#include <memory>
#include <string_view>

#include <gtest/gtest.h>

namespace pdfoxium::integration {
namespace {

bool UsesPinnedBatch(std::string_view suite) {
  // Parameterized suites include their instantiation prefix in the suite name.
  const auto separator=suite.rfind('/');
  if(separator!=suite.npos)suite.remove_prefix(separator+1);
  // These suites explicitly request pinned fixtures. Host-discovery/provider,
  // renderer-flag and unrelated fixture suites keep their own environments.
  for(const char* compatible:{"Text","TextLinks","Forms","FormActions",
      "CoverageScriptAppEvents","CoverageDocumentActionTree","CoverageXfaDataMerge",
      "CoverageXfaScriptContexts","CoverageLegacySecurityAvailability","CoverageFormCalc","CoveragePictures","CoverageXfaFlow","CoverageSomXml","CoverageXfaTextInput","CoverageFormCalcBoundaries","CoverageXfaValidationBinding",
      "CoverageXfaLayout","CoverageXfaBindingLayout","CoverageXfaSchema","CoverageXfaHostCallbacks",
      "CoverageLocaleService","CoverageBarcodeVariants","CoverageBarcodeBoundaries",
      "CoverageXfaLayoutVariants","CoverageXfaTraversalBidi",
      "CoverageWidgetCaptureNavigation","CoverageWidgetScrollFocus",
      "CoverageParserAvailabilityRecovery","CoverageStreamDocumentErrors",
      "CoverageJavascriptServiceRecovery","CoverageScriptFieldActions",
      "CoverageRichtextCodecRecovery","CoverageXfaEventRecovery",
      "CoverageXfaPictureModels","CoverageXfaTimezoneBoundaries",
      "CoverageXfaMergePageRecovery","CoverageXfaLayoutRecovery",
      "CoverageParserRangeSecurity",
      "CoverageXfaLayoutConstraints",
      "CoverageFormAppearanceConstraints",
      "CoverageXfaTextConstraints",
      "CoverageXfaAreaConstraints",
      "CoverageXfaGapHistory",
      "CoverageXfaRichBlockConstraints",
      "CoverageXfaLongListConstraints",
      "CoverageXfaDateEditConstraints",
      "CoverageScriptResolutionConstraints",
      "CoverageTextHitTesting",
      "CoverageXfaPaginationInteractions",
      "CoverageXfaRelayoutSequences",
      "CoverageScrollSelectionSequences",
      "CoverageEncryptedRootRecovery",
      "CoverageAppearanceFontResources",
      "CoverageEditOperationSequences",
      "CoverageFormWindowRecreation",
      "CoverageNestedWidgetScroll",
      "CoverageXfaCloneRebinding",
      "CoverageXfaNamedImages",
      "CoverageXfaPlainParagraph",
      "CoverageXfaHistoryRing",
      "CoverageProgressivePageTreeFallback",
      "CoverageFormCalcScalarConstraints",
      "CoverageWidgetlessFieldConstraints",
      "CoverageOptionalHostServices",
      "CoverageXfaPacketPrototype",
      "CoverageXfaCustomLocale",
      "CoverageCryptoFilterConstraints",
      "CoverageXfaEventPayloads",
      "PinnedTestFonts","ComparisonRendering"}) {
    if(suite==compatible)return true;
  }
  return false;
}

class PinnedFontBatches final:public ::testing::EmptyTestEventListener {
 public:
  void OnTestSuiteStart(const ::testing::TestSuite& suite) override {
    if(!UsesPinnedBatch(suite.name())) {
      Reset();
    } else if(!fonts_) {
      // Version1 enumeration serves both ordinary mapping and XFA aliases.
      // Per-scenario scopes borrow this state, avoiding native PDFium's repeated
      // accumulation and parsing of the installed-font catalogue on restoration.
      fonts_=std::make_unique<ScopedPinnedTestFonts>(/*enumerate_xfa=*/true);
    }
  }
  void OnTestProgramEnd(const ::testing::UnitTest&) override { Reset(); }
  void Reset() { fonts_.reset(); }

 private:
  std::unique_ptr<ScopedPinnedTestFonts> fonts_;
};

PinnedFontBatches* installed_batches=nullptr;

}  // namespace

void InstallPinnedFontBatches() {
  if(installed_batches)return;
  auto listener=std::make_unique<PinnedFontBatches>();
  installed_batches=listener.get();
  ::testing::UnitTest::GetInstance()->listeners().Append(listener.release());
}

void ResetPinnedFontBatches() {
  if(installed_batches)installed_batches->Reset();
}

}  // namespace pdfoxium::integration
