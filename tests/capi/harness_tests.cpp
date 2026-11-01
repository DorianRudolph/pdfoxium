#include "harness.h"

#include <exception>
#include <thread>

#include <gtest/gtest.h>

namespace pdfoxium::integration {

TEST(Harness, ResolvesCompleteTypedApiForBothLibraries) {
  EXPECT_EQ(kApiFunctionCount, 470u);
  EXPECT_FALSE(std::filesystem::equivalent(libraries().native.path(),
                                         libraries().rust.path()));
  for (const Library* library : {&libraries().native, &libraries().rust}) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
#define PDFIUM_API(name) EXPECT_NE(api.name, nullptr) << #name;
#include "api_functions.inc"
#undef PDFIUM_API
    EXPECT_EQ(&api, &library->api());
  }
}

TEST(Harness, RejectsMissingLibraryWithPathDiagnostic) {
  const auto path = libraries().native.path() / "does-not-exist";
  try {
    Library missing("missing-test", path);
    FAIL() << "An invalid library path was accepted";
  } catch (const std::runtime_error& error) {
    EXPECT_NE(std::string(error.what()).find(path.u8string()), std::string::npos);
    EXPECT_NE(std::string(error.what()).find("missing-test"), std::string::npos);
  }
}

TEST(Harness, RejectsApiAccessFromAnotherThread) {
  std::exception_ptr failure;
  std::thread worker([&] {
    try {
      libraries().native.api();
    } catch (...) {
      failure = std::current_exception();
    }
  });
  worker.join();
  ASSERT_NE(failure, nullptr);
  EXPECT_THROW(std::rethrow_exception(failure), std::logic_error);
  // The owner and initialized session remain usable after the rejected access.
  EXPECT_NE(libraries().native.api().FPDF_CreateNewDocument, nullptr);
}

}  // namespace pdfoxium::integration
